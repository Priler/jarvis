use std::sync::mpsc::{self, Receiver};
use std::thread;

use jarvis_core::{
    audio_processing, commands, config,
    i18n, ipc::{self, IpcEvent},
    intent, listener, recorder, slots,
    voices, COMMANDS_LIST,
};
use std::time::Duration;

use std::sync::atomic::Ordering;
use std::time::SystemTime;
use crate::stt_worker::{AudioFrame, SttCmd, SttEvent, ACTIVE_COMMAND_SESSION, LAST_IPC_ACTIVITY_MS};
use crate::voice_intelligence as vi;
use crate::bus::BusEvent;
use crate::platform::AppPlatform;
use crate::scheduler::JobType;

use crate::should_stop;

/// Thin wrapper around ipc::send that stamps the IPC heartbeat.
/// All IPC sends in this module go through this function.
#[inline]
fn ipc_send(event: IpcEvent) {
    ipc::send(event);
    LAST_IPC_ACTIVITY_MS.store(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        Ordering::Relaxed,
    );
}

pub fn start(text_cmd_rx: Receiver<String>, rt: &tokio::runtime::Runtime) -> Result<(), ()> {
    main_loop(text_cmd_rx, rt)
}

fn main_loop(text_cmd_rx: Receiver<String>, rt: &tokio::runtime::Runtime) -> Result<(), ()> {
    const FRAME_LENGTH: usize = 512;
    const SAMPLE_RATE: usize = 16000;

    let mut frame_buffer = vec![0i16; FRAME_LENGTH];

    // ── channel setup ─────────────────────────────────────────────────────────
    let (frame_tx, frame_rx) = mpsc::sync_channel::<AudioFrame>(8);
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<SttCmd>(4);
    let (event_tx, event_rx) = mpsc::sync_channel::<SttEvent>(16);

    // ── spawn STT worker ──────────────────────────────────────────────────────
    thread::Builder::new()
        .name("stt-worker".into())
        .spawn(move || {
            crate::stt_worker::run(frame_rx, cmd_rx, event_tx, FRAME_LENGTH, SAMPLE_RATE);
        })
        .expect("Failed to spawn STT worker thread");

    // ── Initialize JARVIS OS platform ─────────────────────────────────────────
    let mut platform = AppPlatform::new();
    platform.start();

    voices::play_greet();

    match recorder::start_recording() {
        Ok(_) => info!(
            "Recording started. Microphone: {}",
            recorder::get_audio_device_name(recorder::get_selected_microphone_index())
        ),
        Err(_) => {
            error!("Cannot start recording.");
            return Err(());
        }
    }

    ipc_send(IpcEvent::Idle);

    // WAV test-mode tracking.
    let mut wav_done_silence: u32 = 0;
    let wav_done_grace = ((1.5 * SAMPLE_RATE as f32) / FRAME_LENGTH as f32) as u32;

    // Loop counter for periodic tasks (screen poll, scheduler tick, health checks).
    let mut loop_count: u32 = 0;

    // ── AUDIO CAPTURE LOOP ────────────────────────────────────────────────────
    loop {
        loop_count = loop_count.wrapping_add(1);

        if should_stop()
            || crate::watchdog::WATCHDOG_SHUTDOWN_REQUEST.load(Ordering::Acquire)
        {
            info!("Stop signal received, shutting down...");
            platform.agents.stop_all();
            voices::play_goodbye();
            ipc_send(IpcEvent::Stopping);
            break;
        }

        // ── Watchdog-requested recorder restart ───────────────────────────────
        if crate::watchdog::RECORDER_RESTART_REQUEST.load(Ordering::Acquire) {
            info!("[APP] Watchdog requested recorder restart — restarting");
            std::thread::sleep(Duration::from_millis(200));
            if recorder::start_recording().is_ok() {
                info!("[APP] Recorder restarted successfully");
            } else {
                error!("[APP] Recorder restart failed — entering degraded mode");
                crate::watchdog::DEGRADED_MODE.store(true, Ordering::Release);
                ipc_send(IpcEvent::Error {
                    message: "Microphone device lost and could not be recovered.".to_string(),
                });
            }
            crate::watchdog::RECORDER_RESTART_REQUEST.store(false, Ordering::Release);
            continue;
        }

        // Text commands injected via the GUI text field.
        if let Ok(text) = text_cmd_rx.try_recv() {
            process_text_command(&text, rt, &mut platform);
            continue;
        }

        // Drain all pending STT events (non-blocking).
        // Also detect if the STT worker thread died (channel Disconnected).
        loop {
            match event_rx.try_recv() {
                Ok(event) => handle_stt_event(event, &cmd_tx, rt, &mut platform),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if !crate::watchdog::DEGRADED_MODE.load(Ordering::Relaxed) {
                        error!("[APP] STT worker channel disconnected — worker may have panicked");
                        crate::recovery::execute_l3_degraded_mode("stt_worker_disconnected");
                    }
                    break;
                }
            }
        }

        // Scheduler tick every ~1.6 s (50 frames × 32 ms).
        if loop_count % 50 == 0 {
            let fired = platform.scheduler.tick();
            for job in fired {
                handle_scheduled_job(job, rt, &mut platform);
            }
        }

        // Screen context poll + agent health check every ~3.2 s (100 frames).
        if loop_count % 100 == 0 {
            if let Some(ctx) = platform.perception.poll_screen() {
                ipc_send(IpcEvent::ScreenContext { window_title: ctx.window_title.clone() });
            }
            platform.agents.run_health_checks();
            platform.agents.recover_failed();
        }

        // Block until the next audio frame (~32 ms at 16 kHz / 512 samples).
        recorder::read_microphone(&mut frame_buffer);
        let processed = audio_processing::process(&frame_buffer);

        // WAV test-mode: auto-terminate after WAV is exhausted + grace period.
        if recorder::is_wav_mode() && recorder::is_wav_done() {
            if !processed.is_voice {
                wav_done_silence += 1;
                if wav_done_silence >= wav_done_grace {
                    info!("[AUDIO_TEST] WAV replay complete — shutting down");
                    info!("[WAKE] Total frames sent to Rustpotter: {}", listener::get_frame_count());
                    info!("[WAKE] Max score seen: {:.3}", listener::get_max_score());
                    info!("[WAKE] Min score threshold: {:.3}", config::RUSPOTTER_MIN_SCORE);
                    break;
                }
            } else {
                wav_done_silence = 0;
            }
        }

        // Rustpotter: fast wake-word detection stays on the capture thread.
        let rustpotter_wake = listener::data_callback(&frame_buffer).is_some();

        if frame_tx.try_send(AudioFrame {
            pcm: frame_buffer.clone(),
            is_voice: processed.is_voice,
            rustpotter_wake,
            vad_rms: {
                let sum: f64 = frame_buffer.iter().map(|&s| (s as f64).powi(2)).sum();
                (sum / frame_buffer.len() as f64).sqrt() as f32
            },
        }).is_err() {
            debug!("[AUDIO] STT worker queue full — dropping frame");
        }
    }

    drop(frame_tx);
    drop(cmd_tx);

    recorder::stop_recording().ok();
    ipc_send(IpcEvent::Stopping);

    if recorder::is_wav_mode() {
        if crate::testing::is_active() {
            let code = crate::testing::harness::TestHarness::on_replay_complete();
            std::process::exit(code);
        }
        std::process::exit(0);
    }

    Ok(())
}

// ── STT event handler ─────────────────────────────────────────────────────────

fn handle_stt_event(
    event: SttEvent,
    cmd_tx: &mpsc::SyncSender<SttCmd>,
    rt: &tokio::runtime::Runtime,
    platform: &mut AppPlatform,
) {
    match event {
        SttEvent::WakeDetected { session_id } => {
            if crate::is_muted() {
                info!("[WAKE S:{}] muted — suppressing wake", session_id);
                let _ = cmd_tx.send(SttCmd::Idle);
                return;
            }
            info!("[WAKE S:{}] confirmed → playing reply, entering Listening", session_id);
            ipc_send(IpcEvent::WakeWordDetected);
            let now = vi::now_ms();
            vi::RESPONSE_STARTED_AT_MS.store(now, Ordering::Relaxed);
            let wake_at = vi::WAKE_DETECTED_AT_MS.load(Ordering::Relaxed);
            if wake_at > 0 {
                let wake_latency = now.saturating_sub(wake_at);
                vi::update_running_avg(&vi::AVG_WAKE_LATENCY_MS, wake_latency);
                info!("[LATENCY][WAKE S:{}] wake→listen={}ms avg={}ms",
                    session_id, wake_latency, vi::AVG_WAKE_LATENCY_MS.load(Ordering::Relaxed));
            }
            ipc_send(IpcEvent::CognitionState { state: "listening".to_string() });
            voices::play_reply();
            ipc_send(IpcEvent::Listening);
        }

        SttEvent::SpeechRecognized { text, wake_session_id, cmd_session_id } => {
            // Stale event guard.
            let active_cmd = ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);
            if active_cmd != cmd_session_id {
                warn!("[WARN][CMD] Stale SpeechRecognized ignored cmd_session_id={} active={}",
                    cmd_session_id, active_cmd);
                return;
            }
            // Interrupt early-exit.
            if vi::INTERRUPT_REQUESTED.load(Ordering::Acquire) {
                info!("[INTERRUPT][CMD S:{}] interrupt consumed — skipping execution", cmd_session_id);
                vi::INTERRUPT_REQUESTED.store(false, Ordering::Release);
                let _ = cmd_tx.send(SttCmd::Idle);
                ipc_send(IpcEvent::Idle);
                return;
            }

            vi::EXECUTION_STARTED_AT_MS.store(vi::now_ms(), Ordering::Relaxed);
            info!("[CMD S:{}][WAKE S:{}] received text=\"{}\"", cmd_session_id, wake_session_id, text);
            ipc_send(IpcEvent::SpeechRecognized { text: text.clone() });

            // ── Screen context (multimodal) ────────────────────────────────────
            let screen_hint = platform.perception.screen_domain_hint().map(|s| s.to_string());
            if let Some(ref hint) = screen_hint {
                debug!("[PERCEPTION] Screen domain hint: {}", hint);
            }

            // ── Cognitive pre-processing ───────────────────────────────────────
            ipc_send(IpcEvent::CognitionState { state: "understanding".to_string() });
            let cog_result = platform.cognitive.process(&text);

            // Publish voice intent to bus.
            platform.bus.publish(BusEvent::VoiceIntent {
                text: text.clone(),
                domain: cog_result.enriched_intent.domain.as_str().to_string(),
                confidence: cog_result.enriched_intent.confidence,
            });

            if let Some(ref ctx) = cog_result.memory_context {
                info!("[COGNITIVE][CMD S:{}] memory_hit context=\"{}\"", cmd_session_id, ctx);
                ipc_send(IpcEvent::MemoryRecalled { summary: ctx.clone() });
            }

            platform.cognitive.log_decision(
                &text,
                &cog_result.enriched_intent.domain,
                &format!("entities={} urgency={:?} ctx_dep={} memory={} screen={:?}",
                    cog_result.enriched_intent.entities.len(),
                    cog_result.enriched_intent.urgency,
                    cog_result.enriched_intent.context_dependent,
                    cog_result.memory_context.is_some(),
                    screen_hint,
                ),
                if cog_result.clarification_needed.is_some() { "clarification" } else { "execute" },
            );

            // Clarification gate.
            if let Some((question, options)) = cog_result.clarification_needed {
                info!("[COGNITIVE][CMD S:{}] clarification_needed=\"{}\"", cmd_session_id, question);
                ipc_send(IpcEvent::CognitionState { state: "awaiting_clarification".to_string() });
                ipc_send(IpcEvent::ClarificationNeeded { question, options });
                voices::play_not_found();
                let _ = cmd_tx.send(SttCmd::Idle);
                ipc_send(IpcEvent::Idle);
                return;
            }

            // Multi-step plan notification.
            if cog_result.plan.steps.len() > 1 {
                ipc_send(IpcEvent::PlanStarted {
                    goal: cog_result.plan.goal.clone(),
                    steps: cog_result.plan.step_descriptions(),
                });
            }

            // ── Workflow trigger detection ─────────────────────────────────────
            if let Some(wf_id) = platform.workflows.find_by_voice_phrase(&text) {
                let wf_name = platform.workflows.get(&wf_id)
                    .map(|w| w.name.clone()).unwrap_or_default();
                let wf_steps = platform.workflows.get(&wf_id)
                    .map(|w| w.step_descriptions()).unwrap_or_default();
                let wf_total = wf_steps.len();

                info!("[WORKFLOW][CMD S:{}] Triggered '{}' ({} steps)", cmd_session_id, wf_name, wf_total);
                ipc_send(IpcEvent::WorkflowStarted {
                    id: wf_id.clone(),
                    name: wf_name.clone(),
                    steps: wf_steps,
                });
                platform.bus.publish(BusEvent::WorkflowTriggered { id: wf_id.clone(), name: wf_name });

                match platform.workflows.trigger(&wf_id) {
                    Ok(Some(first_cmd)) => {
                        ipc_send(IpcEvent::CognitionState { state: "executing".to_string() });
                        let before = vi::COMMANDS_MATCHED.load(Ordering::Relaxed);
                        execute_command(&first_cmd, rt, wake_session_id, cmd_session_id, platform, "workflow");
                        let step_ok = vi::COMMANDS_MATCHED.load(Ordering::Relaxed) > before;
                        ipc_send(IpcEvent::WorkflowStepCompleted {
                            workflow_id: wf_id.clone(), step: first_cmd, index: 0, total: wf_total, success: step_ok,
                        });
                        platform.workflows.advance(step_ok);
                    }
                    Ok(None) => {}
                    Err(e) => error!("[WORKFLOW] Trigger failed: {}", e),
                }

                platform.cognitive.observe(&cog_result.enriched_intent, true);
                let _ = cmd_tx.send(SttCmd::Idle);
                ipc_send(IpcEvent::Idle);
                return;
            }

            // ── Regular execution ──────────────────────────────────────────────
            ipc_send(IpcEvent::CognitionState { state: "executing".to_string() });
            let before_matched = vi::COMMANDS_MATCHED.load(Ordering::Relaxed);
            let should_chain = execute_command(&text, rt, wake_session_id, cmd_session_id, platform, "voice");
            let exec_success = vi::COMMANDS_MATCHED.load(Ordering::Relaxed) > before_matched;

            platform.cognitive.observe(&cog_result.enriched_intent, exec_success);
            platform.bus.publish(BusEvent::CommandCompleted {
                intent_id: cog_result.enriched_intent.matched_intent_id
                    .as_deref().unwrap_or(&text).to_string(),
                success: exec_success,
            });

            let _ = cmd_tx.send(if should_chain { SttCmd::Chain } else { SttCmd::Idle });
            if should_chain {
                ipc_send(IpcEvent::Listening);
            }
        }

        SttEvent::CommandTimeout { wake_session_id, timeout_type } => {
            info!("[TIMEOUT][WAKE S:{}] type={} → Idle", wake_session_id, timeout_type);
            platform.cognitive.on_session_end();
            ipc_send(IpcEvent::CognitionState { state: "idle".to_string() });
            ipc_send(IpcEvent::Idle);
        }
    }
}

// ── Scheduled job handler ─────────────────────────────────────────────────────

fn handle_scheduled_job(
    job: crate::scheduler::ScheduledJob,
    rt: &tokio::runtime::Runtime,
    platform: &mut AppPlatform,
) {
    info!("[SCHEDULER] Firing job '{}' type={}", job.id, job.job_type.as_str());
    match job.job_type {
        JobType::WorkflowTrigger { ref workflow_id } => {
            let wf_name = platform.workflows.get(workflow_id)
                .map(|w| w.name.clone()).unwrap_or_default();
            let wf_steps = platform.workflows.get(workflow_id)
                .map(|w| w.step_descriptions()).unwrap_or_default();
            ipc_send(IpcEvent::WorkflowStarted {
                id: workflow_id.clone(), name: wf_name.clone(), steps: wf_steps,
            });
            match platform.workflows.trigger(workflow_id) {
                Ok(Some(cmd)) => { execute_command(&cmd, rt, 0, 0, platform, "scheduler"); }
                Ok(None) => {}
                Err(e) => error!("[SCHEDULER] Workflow trigger failed: {}", e),
            }
        }
        JobType::CognitiveReport => {
            platform.cognitive.emit_health_report();
        }
        JobType::HealthCheck => {
            platform.agents.run_health_checks();
        }
        _ => {}
    }
}

// ── Command execution ─────────────────────────────────────────────────────────

fn process_text_command(text: &str, rt: &tokio::runtime::Runtime, platform: &mut AppPlatform) {
    info!("[CMD] text_command path text=\"{}\"", text);
    ipc_send(IpcEvent::SpeechRecognized { text: text.to_string() });

    let mut filtered = text.to_lowercase();
    for tbr in config::get_phrases_to_remove(&i18n::get_language()) {
        filtered = filtered.replace(*tbr, "");
    }
    let filtered = filtered.trim().to_string();
    if filtered.is_empty() {
        ipc_send(IpcEvent::Idle);
        return;
    }
    execute_command(&filtered, rt, 0, 0, platform, "text");
}

/// Execute a matched command. Returns true if chaining should continue.
/// `wake_sid` and `cmd_sid` are 0 for text-command (non-STT) invocations.
fn execute_command(
    text: &str,
    rt: &tokio::runtime::Runtime,
    wake_sid: u64,
    cmd_sid: u64,
    platform: &mut AppPlatform,
    source: &str,
) -> bool {
    let commands_lock = match COMMANDS_LIST.get() {
        Some(lock) => lock,
        None => {
            ipc_send(IpcEvent::Error { message: "Commands not loaded".to_string() });
            ipc_send(IpcEvent::Idle);
            return false;
        }
    };
    let commands_list = commands_lock.read();

    let mut matched_intent_id: Option<String> = None;

    let cmd_result = if let Some((intent_id, confidence)) = rt.block_on(intent::classify(text)) {
        if confidence < vi::CONFIDENCE_THRESHOLD_REJECT as f64 {
            info!("[CONFIDENCE][CMD S:{}][WAKE S:{}] REJECT confidence={:.2} intent=\"{}\"",
                cmd_sid, wake_sid, confidence, intent_id);
            vi::LOW_CONFIDENCE_DEFLECTIONS.fetch_add(1, Ordering::Relaxed);
            None
        } else {
            if confidence < vi::CONFIDENCE_THRESHOLD_WARN as f64 {
                info!("[CONFIDENCE][CMD S:{}][WAKE S:{}] WARN confidence={:.2} — proceeding",
                    cmd_sid, wake_sid, confidence);
            }
            info!("[MATCH][CMD S:{}][WAKE S:{}] type=intent intent_id=\"{}\" confidence={:.2}",
                cmd_sid, wake_sid, intent_id, confidence);
            matched_intent_id = Some(intent_id.clone());
            intent::get_command_by_intent(&*commands_list, &intent_id)
        }
    } else {
        let result = commands::fetch_command(text, &*commands_list);
        if result.is_some() {
            info!("[MATCH][CMD S:{}][WAKE S:{}] type=levenshtein candidate=\"{}\"",
                cmd_sid, wake_sid, text);
        } else {
            info!("[MATCH][CMD S:{}][WAKE S:{}] result=none text=\"{}\"",
                cmd_sid, wake_sid, text);
        }
        result
    };

    if let Some((cmd_path, cmd_config)) = cmd_result {
        // ── Governance check ──────────────────────────────────────────────────
        let gov = platform.governance.check_command(
            &cmd_config.cmd_type,
            &cmd_config.cli_cmd,
            &cmd_config.sandbox,
            source,
        );

        if !gov.allowed {
            warn!("[GOVERNANCE][CMD S:{}] BLOCKED risk={} reason=\"{}\"",
                cmd_sid, gov.risk_level.as_str(), gov.reason);
            platform.bus.publish(BusEvent::GovernanceAlert {
                risk_level: gov.risk_level.clone(),
                action: cmd_config.cli_cmd.clone(),
                blocked: true,
            });
            ipc_send(IpcEvent::GovernanceAlert {
                risk_level: gov.risk_level.as_str().to_string(),
                action: cmd_config.cli_cmd.clone(),
                blocked: true,
            });
            voices::play_error();
            ipc_send(IpcEvent::Error { message: gov.reason.clone() });
            ipc_send(IpcEvent::Idle);
            return false;
        }

        if gov.requires_confirmation && !commands::requires_confirmation(&cmd_config) {
            // Governance wants confirmation even if the command doesn't set `confirm`.
            warn!("[GOVERNANCE][CMD S:{}] risk={} — escalating to confirmation",
                cmd_sid, gov.risk_level.as_str());
            platform.bus.publish(BusEvent::GovernanceAlert {
                risk_level: gov.risk_level.clone(),
                action: cmd_config.cli_cmd.clone(),
                blocked: false,
            });
        }

        info!("[CMD S:{}][WAKE S:{}] EXECUTE id=\"{}\" path={:?}",
            cmd_sid, wake_sid, cmd_config.id, cmd_path);

        // Publish dispatch event.
        platform.bus.publish(BusEvent::CommandDispatched {
            intent_id: matched_intent_id.clone().unwrap_or_else(|| text.to_string()),
            text: text.to_string(),
        });

        let extracted_slots = if !cmd_config.slots.is_empty() {
            let s = slots::extract(text, &cmd_config.slots);
            if !s.is_empty() {
                info!("[CMD S:{}] slots extracted: {:?}", cmd_sid, s);
            }
            Some(s)
        } else {
            None
        };

        if commands::requires_confirmation(&cmd_config) || gov.requires_confirmation {
            let cmd_str = if cmd_config.cli_args.is_empty() {
                cmd_config.cli_cmd.clone()
            } else {
                format!("{} {}", cmd_config.cli_cmd, cmd_config.cli_args.join(" "))
            };
            info!("[CMD S:{}][WAKE S:{}] awaiting_confirmation cmd=\"{}\"", cmd_sid, wake_sid, cmd_str);
            commands::store_pending_command(cmd_path, &cmd_config);
            ipc_send(IpcEvent::ConfirmationRequired {
                id: cmd_config.id.clone(),
                description: cmd_config.description.clone(),
                cmd: cmd_str,
            });
            ipc_send(IpcEvent::Idle);
            return false;
        }

        match commands::execute_command(&cmd_path, &cmd_config, Some(text), extracted_slots.as_ref()) {
            Ok(chain) => {
                info!("[CMD S:{}][WAKE S:{}] result=success chain={} id=\"{}\"",
                    cmd_sid, wake_sid, chain, cmd_config.id);
                info!("[STATE] Executing → Speaking");
                vi::COMMANDS_MATCHED.fetch_add(1, Ordering::Relaxed);
                if let Some(ref iid) = matched_intent_id {
                    vi::VOICE_CTX.lock().record_command(text, iid);
                }
                let wake_at = vi::WAKE_DETECTED_AT_MS.load(Ordering::Relaxed);
                let exec_at = vi::EXECUTION_STARTED_AT_MS.load(Ordering::Relaxed);
                if wake_at > 0 && exec_at >= wake_at {
                    let rt_ms = exec_at.saturating_sub(wake_at);
                    vi::LAST_ROUNDTRIP_MS.store(rt_ms, Ordering::Relaxed);
                    vi::update_running_avg(&vi::AVG_ROUNDTRIP_MS, rt_ms);
                    info!("[LATENCY][CMD S:{}] roundtrip={}ms avg={}ms",
                        cmd_sid, rt_ms, vi::AVG_ROUNDTRIP_MS.load(Ordering::Relaxed));
                }
                voices::play_random_from(cmd_config.get_sounds(&i18n::get_language()).as_slice());
                ipc_send(IpcEvent::CommandExecuted { id: cmd_config.id.clone(), success: true });
                if !chain {
                    info!("[STATE][WAKE S:{}] Executing → Idle", wake_sid);
                    ipc_send(IpcEvent::Idle);
                }
                return chain;
            }
            Err(msg) => {
                error!("[CMD S:{}][WAKE S:{}] result=error msg=\"{}\"", cmd_sid, wake_sid, msg);
                voices::play_error();
                ipc_send(IpcEvent::CommandExecuted { id: cmd_config.id.clone(), success: false });
                ipc_send(IpcEvent::Error { message: msg.to_string() });
            }
        }
    } else {
        info!("[CMD S:{}][WAKE S:{}] result=no_match text=\"{}\"", cmd_sid, wake_sid, text);
        vi::COMMANDS_UNMATCHED.fetch_add(1, Ordering::Relaxed);
        voices::play_not_found();
        ipc_send(IpcEvent::Error { message: format!("Command not found: {}", text) });
    }

    ipc_send(IpcEvent::Idle);
    false
}

pub fn close(code: i32) {
    info!("Closing application.");
    voices::play_goodbye();
    ipc_send(IpcEvent::Stopping);
    std::process::exit(code);
}
