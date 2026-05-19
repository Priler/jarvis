use std::sync::mpsc::{self, Receiver};
use std::thread;

use jarvis_core::{
    audio_processing, commands, config,
    i18n, ipc::{self, IpcEvent},
    intent, listener, recorder, slots,
    voices, COMMANDS_LIST,
};

use crate::stt_worker::{AudioFrame, SttCmd, SttEvent};

use crate::should_stop;

pub fn start(text_cmd_rx: Receiver<String>, rt: &tokio::runtime::Runtime) -> Result<(), ()> {
    main_loop(text_cmd_rx, rt)
}

fn main_loop(text_cmd_rx: Receiver<String>, rt: &tokio::runtime::Runtime) -> Result<(), ()> {
    const FRAME_LENGTH: usize = 512;
    const SAMPLE_RATE: usize = 16000;

    let mut frame_buffer = vec![0i16; FRAME_LENGTH];

    // ── channel setup ─────────────────────────────────────────────────────────
    //
    // audio thread → STT worker: bounded so a slow worker never stalls
    // the capture loop (frames are dropped rather than blocking the mic).
    let (frame_tx, frame_rx) = mpsc::sync_channel::<AudioFrame>(8);
    // main thread → STT worker: chain/idle decisions after command execution.
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<SttCmd>(4);
    // STT worker → main thread: recognition events.
    let (event_tx, event_rx) = mpsc::sync_channel::<SttEvent>(16);

    // ── spawn STT worker ──────────────────────────────────────────────────────
    thread::Builder::new()
        .name("stt-worker".into())
        .spawn(move || {
            crate::stt_worker::run(frame_rx, cmd_rx, event_tx, FRAME_LENGTH, SAMPLE_RATE);
        })
        .expect("Failed to spawn STT worker thread");

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

    ipc::send(IpcEvent::Idle);

    // WAV test-mode tracking.
    let mut wav_done_silence: u32 = 0;
    let wav_done_grace = ((1.5 * SAMPLE_RATE as f32) / FRAME_LENGTH as f32) as u32;

    // ── AUDIO CAPTURE LOOP ────────────────────────────────────────────────────
    //
    // This thread owns the microphone and Rustpotter (fast: ~460 µs/frame).
    // All Vosk work (wake grammar ~10-12 ms, speech ~1.6 ms) runs on the
    // stt-worker thread, so the capture loop is never blocked by STT decode.
    loop {
        if should_stop() {
            info!("Stop signal received, shutting down...");
            voices::play_goodbye();
            ipc::send(IpcEvent::Stopping);
            break;
        }

        // Text commands injected via the GUI text field.
        if let Ok(text) = text_cmd_rx.try_recv() {
            process_text_command(&text, rt);
            continue;
        }

        // Drain all pending STT events (non-blocking).
        while let Ok(event) = event_rx.try_recv() {
            handle_stt_event(event, &cmd_tx, rt);
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

        // Forward frame + metadata to the STT worker.  try_send never blocks:
        // if the worker is behind, the oldest unprocessed frame is effectively
        // skipped rather than stalling the microphone.
        if frame_tx.try_send(AudioFrame {
            pcm: frame_buffer.clone(),
            is_voice: processed.is_voice,
            rustpotter_wake,
        }).is_err() {
            debug!("[AUDIO] STT worker queue full — dropping frame");
        }
    }

    // Drop the frame sender to signal the STT worker to exit.
    drop(frame_tx);
    drop(cmd_tx);

    recorder::stop_recording().ok();
    ipc::send(IpcEvent::Stopping);
    Ok(())
}

// ── STT event handler ─────────────────────────────────────────────────────────

fn handle_stt_event(
    event: SttEvent,
    cmd_tx: &mpsc::SyncSender<SttCmd>,
    rt: &tokio::runtime::Runtime,
) {
    match event {
        SttEvent::WakeDetected => {
            if crate::is_muted() {
                info!("[MUTED] Wake word detected but muted — ignoring");
                let _ = cmd_tx.send(SttCmd::Idle);
                return;
            }
            info!("Wake word activated!");
            ipc::send(IpcEvent::WakeWordDetected);
            voices::play_reply();
            ipc::send(IpcEvent::Listening);
        }

        SttEvent::SpeechRecognized(text) => {
            ipc::send(IpcEvent::SpeechRecognized { text: text.clone() });
            let should_chain = execute_command(&text, rt);
            let _ = cmd_tx.send(if should_chain { SttCmd::Chain } else { SttCmd::Idle });
            if !should_chain {
                ipc::send(IpcEvent::Idle);
            } else {
                ipc::send(IpcEvent::Listening);
            }
        }

        SttEvent::CommandTimeout => {
            info!("[STT] Command timeout — returning to idle");
            ipc::send(IpcEvent::Idle);
        }
    }
}

// ── Command execution ─────────────────────────────────────────────────────────

fn process_text_command(text: &str, rt: &tokio::runtime::Runtime) {
    info!("Processing text command: {}", text);
    ipc::send(IpcEvent::SpeechRecognized { text: text.to_string() });

    let mut filtered = text.to_lowercase();
    for tbr in config::get_phrases_to_remove(&i18n::get_language()) {
        filtered = filtered.replace(*tbr, "");
    }
    let filtered = filtered.trim();
    if filtered.is_empty() {
        ipc::send(IpcEvent::Idle);
        return;
    }
    execute_command(filtered, rt);
}

/// Execute a matched command. Returns true if chaining should continue.
fn execute_command(text: &str, rt: &tokio::runtime::Runtime) -> bool {
    let commands_lock = match COMMANDS_LIST.get() {
        Some(lock) => lock,
        None => {
            ipc::send(IpcEvent::Error {
                message: "Commands not loaded".to_string(),
            });
            ipc::send(IpcEvent::Idle);
            return false;
        }
    };
    let commands_list = commands_lock.read();

    let cmd_result = if let Some((intent_id, confidence)) = rt.block_on(intent::classify(text)) {
        info!("Intent recognized: {} (confidence: {:.2})", intent_id, confidence);
        intent::get_command_by_intent(&*commands_list, &intent_id)
    } else {
        info!("Intent not recognized, trying levenshtein fallback...");
        commands::fetch_command(text, &*commands_list)
    };

    if let Some((cmd_path, cmd_config)) = cmd_result {
        info!("[COMMAND] Matched: {:?}", cmd_path);

        let extracted_slots = if !cmd_config.slots.is_empty() {
            let s = slots::extract(text, &cmd_config.slots);
            if !s.is_empty() {
                info!("Extracted slots: {:?}", s);
            }
            Some(s)
        } else {
            None
        };

        if commands::requires_confirmation(&cmd_config) {
            let cmd_str = if cmd_config.cli_args.is_empty() {
                cmd_config.cli_cmd.clone()
            } else {
                format!("{} {}", cmd_config.cli_cmd, cmd_config.cli_args.join(" "))
            };
            commands::store_pending_command(cmd_path, &cmd_config);
            ipc::send(IpcEvent::ConfirmationRequired {
                id: cmd_config.id.clone(),
                description: cmd_config.description.clone(),
                cmd: cmd_str,
            });
            ipc::send(IpcEvent::Idle);
            return false;
        }

        match commands::execute_command(&cmd_path, &cmd_config, Some(text), extracted_slots.as_ref()) {
            Ok(chain) => {
                info!("[COMMAND] Executed successfully: {}", cmd_config.id);
                voices::play_random_from(cmd_config.get_sounds(&i18n::get_language()).as_slice());
                ipc::send(IpcEvent::CommandExecuted {
                    id: cmd_config.id.clone(),
                    success: true,
                });
                return chain;
            }
            Err(msg) => {
                error!("Error executing command: {}", msg);
                voices::play_error();
                ipc::send(IpcEvent::CommandExecuted {
                    id: cmd_config.id.clone(),
                    success: false,
                });
                ipc::send(IpcEvent::Error { message: msg.to_string() });
            }
        }
    } else {
        info!("No command found for: {}", text);
        voices::play_not_found();
        ipc::send(IpcEvent::Error {
            message: format!("Command not found: {}", text),
        });
    }

    ipc::send(IpcEvent::Idle);
    false
}

pub fn close(code: i32) {
    info!("Closing application.");
    voices::play_goodbye();
    ipc::send(IpcEvent::Stopping);
    std::process::exit(code);
}
