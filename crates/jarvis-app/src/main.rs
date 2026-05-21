use jarvis_core::slots;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

// include core
use jarvis_core::{
    audio, audio_processing, commands, config, db, listener, recorder, stt, intent,
    ipc::{self, IpcAction, IpcEvent},
    i18n, voices, models,
    APP_CONFIG_DIR, APP_LOG_DIR, COMMANDS_LIST, DB,
};
use parking_lot::RwLock;

// include log
#[macro_use]
extern crate simple_log;
mod log;

// include app
mod adaptive_threshold;
mod app;
mod agents;
mod assistant_voice_fingerprint;
mod bus;
mod cognitive;
mod confidence_fusion;
mod environment_profile;
mod failures;
mod governance;
mod health;
mod perception;
mod platform;
mod plugin;
mod scheduler;
mod adaptive_drift_detector;
mod ai_safety_runtime;
mod autonomous_runtime_observability;
mod desktop_snapshot;
mod dialog_detector;
mod contextual_task_memory;
mod environment_reasoner;
mod execution_graph_runtime;
mod execution_journal;
mod execution_recovery;
mod execution_sandbox;
mod execution_verifier;
mod hallucination_guard_v2;
mod intent_confidence;
mod llm_config;
mod llm_provider;
mod llm_runtime;
mod llm_session;
mod multimodal_observability;
mod multimodal_safety_runtime;
mod multimodal_verification;
mod ocr_runtime;
mod orchestration_runtime;
mod recovery;
mod runtime_bus;
mod runtime_health;
mod runtime_modes;
mod runtime_observability;
mod runtime_recovery;
mod runtime_watchdog;
mod planner_v2;
mod screen_capture;
mod screen_state_journal;
mod semantic_intent;
mod semantic_runtime_observability;
mod service_observability;
mod service_recovery;
mod service_watchdogs;
mod task_graph;
mod tool_capabilities;
mod ui_interaction_runtime;
mod ui_state;
mod visual_verifier;
mod window_graph;
mod world_state;
mod active_observer;
mod anomaly_detector;
mod attention_runtime;
mod cognitive_memory;
mod cognitive_observability;
mod cognitive_safety;
mod cognitive_tick;
mod cognition_loop;
mod cognition_scheduler;
mod goal_runtime;
mod persistent_reasoner;
mod persistent_world_model;
mod predictive_reasoner;
mod reflection_runtime;
mod task_continuity;
mod workflow_learning;
mod world_state_journal;
mod autonomous_learning_safety;
mod behavior_adaptation;
mod cognitive_drift_control;
mod cognitive_evolution;
mod execution_quality;
mod failure_pattern_analyzer;
mod feedback_loop;
mod long_horizon_goals;
mod safe_adaptation;
mod self_evaluation;
mod self_improvement_runtime;
mod strategic_observability;
mod strategy_evaluator;
mod strategy_optimizer;
mod workflow_optimizer;
mod causal_reasoner;
mod cognitive_confidence;
mod cognitive_stability;
mod counterfactual_runtime;
mod future_state_simulator;
mod meta_cognition_runtime;
mod meta_cognition_safety;
mod meta_reflection;
mod meta_strategy_optimizer;
mod reasoning_analyzer;
mod strategic_arbitration;
mod strategy_simulator;
mod uncertainty_engine;
// Phase 18 — live meta-cognition runtime integration
mod cognitive_watchdog;
mod live_counterfactuals;
mod live_meta_loop;
mod live_meta_observability;
mod live_strategy_arbitration;
mod live_uncertainty;
mod meta_event_bus;
mod meta_memory_fusion;
mod meta_scheduler;
// Phase 19 — hierarchical generalized cognition runtime
mod cognition_coordinator;
mod cognition_layers;
mod cognitive_router;
mod generalized_observability;
mod generalized_planner;
mod hierarchical_memory;
mod hierarchical_runtime;
mod hierarchical_safety;
mod hierarchical_scheduler;
mod long_horizon_reasoning;
mod meta_layer;
mod priority_runtime;
mod reactive_layer;
mod resource_reasoner;
mod resource_scheduler;
mod strategic_layer;
mod supervisory_layer;
mod tactical_layer;
mod tool_executor;
mod tool_runtime;
mod stt_worker;
mod testing;
mod voice_intelligence;
mod watchdog;
mod workflows;

// include tray
// @TODO. macOS currently not supported for tray functionality.
#[cfg(not(target_os = "macos"))]
mod tray;

static SHOULD_STOP: AtomicBool = AtomicBool::new(false);
static MUTED: AtomicBool = AtomicBool::new(false);

fn main() -> Result<(), String> {
    // initialize directories
    config::init_dirs()?;

    // initialize logging
    log::init_logging()?;

    // ── Corpus validation dispatch ────────────────────────────────────────────
    if let Some(cfg) = testing::corpus_runner::parse_corpus_validation_command() {
        let report = testing::corpus_runner::run_corpus_validation(&cfg);
        testing::corpus_runner::print_corpus_summary(&report);
        if let Ok(json) = serde_json::to_string_pretty(&report) {
            let out = cfg.output_dir.join("corpus_run_report.json");
            let _ = std::fs::write(&out, json);
            println!("[CORPUS] Full report: {:?}", out);
        }
        let exit_code = match report.certification {
            testing::corpus_runner::CertificationDecision::ProductionReady => 0,
            testing::corpus_runner::CertificationDecision::LimitedReady    => 2,
            testing::corpus_runner::CertificationDecision::NotReady        => 1,
        };
        std::process::exit(exit_code);
    }

    // ── Replay / validation dispatch ──────────────────────────────────────────
    if let Some(cmd) = testing::replay::parse_replay_command() {
        match cmd {
            testing::replay::ReplayCommand::Single { wav_path, output_path, accelerated } => {
                recorder::set_accelerated(accelerated);
                if let Err(e) = testing::replay::setup_single(&wav_path, output_path) {
                    eprintln!("[REPLAY] {}", e);
                    std::process::exit(2);
                }
                // Falls through to normal startup; app.rs WAV exit calls on_replay_complete().
            }
            testing::replay::ReplayCommand::Dir { dir, output_dir, accelerated } => {
                let report = testing::harness::run_dir(&dir, &output_dir, accelerated);
                testing::replay::print_dir_summary(&report);
                std::process::exit(if report.failed > 0 { 1 } else { 0 });
            }
            testing::replay::ReplayCommand::Batch { yaml_path, output_dir, accelerated } => {
                let report = testing::harness::run_batch(&yaml_path, &output_dir, accelerated);
                testing::replay::print_dir_summary(&report);
                std::process::exit(if report.failed > 0 { 1 } else { 0 });
            }
            testing::replay::ReplayCommand::StressReplay { path, iterations, output_dir, accelerated } => {
                let report = if path.is_dir() {
                    testing::harness::run_stress_dir(&path, iterations, &output_dir, accelerated)
                } else {
                    testing::harness::run_stress(&path, iterations, &output_dir, accelerated)
                };
                testing::replay::print_dir_summary(&report);
                std::process::exit(if report.failed > 0 { 1 } else { 0 });
            }
            testing::replay::ReplayCommand::BackgroundValidation { dir, output_dir, accelerated } => {
                let stats = testing::statistical::run_background_validation(
                    &dir, &output_dir, accelerated,
                );
                println!("[BGVAL] === Background Validation Results ===");
                println!("[BGVAL] Files processed : {}", stats.total_files);
                println!("[BGVAL] Total audio     : {:.2}h", stats.total_duration_hours);
                println!("[BGVAL] False wakes     : {}", stats.false_wakes);
                println!("[BGVAL] False wakes/h   : {:.3}", stats.false_wakes_per_hour);
                println!("[BGVAL] Ghost commands  : {}", stats.ghost_commands);
                println!("[BGVAL] Ghost cmds/h    : {:.3}", stats.ghost_commands_per_hour);
                println!("[BGVAL] Dup activations : {}", stats.duplicate_activation_events);
                // Write JSON report
                if let Ok(json) = serde_json::to_string_pretty(&stats) {
                    let out = output_dir.join("background_validation.json");
                    let _ = std::fs::write(&out, json);
                    println!("[BGVAL] Report: {:?}", out);
                }
                std::process::exit(if stats.false_wakes > 0 { 1 } else { 0 });
            }
        }
    }

    // log some base info
    info!("Starting Jarvis v{} ...", config::APP_VERSION.unwrap());
    info!("Config directory is: {}", APP_CONFIG_DIR.get().unwrap().display());
    info!("Log directory is: {}", APP_LOG_DIR.get().unwrap().display());

    // initialize settings
    let settings = db::init();

    // set global DB (for core modules that read settings at init time)
    DB.set(settings.arc().clone())
            .expect("DB already initialized");

    // init voices
    let voice_id = settings.lock().voice.clone();
    let language = settings.lock().language.clone();
    if let Err(e) = voices::init(&voice_id, &language) {
        warn!("Failed to init voices: {}", e);
    }

    // init i18n
    i18n::init(&settings.lock().language);

    // init recorder — WAV test mode or live microphone
    let audio_test_path = parse_audio_test_arg();
    if let Some(ref path) = audio_test_path {
        recorder::set_accelerated(testing::replay::parse_accelerated_flag());
        info!("[AUDIO_TEST] WAV test mode enabled: {}", path);
        if let Err(e) = recorder::init_wav(path) {
            error!("[AUDIO_TEST] Failed to initialize WAV source: {}", e);
            app::close(1);
        }
    } else if recorder::init().is_err() {
        app::close(1);
    }

    // shared async runtime for intent classification, IPC, etc.
    // Created early so the IPC action handler can use it before heavy init.
    let rt = Arc::new(
        tokio::runtime::Runtime::new().expect("Failed to create tokio runtime")
    );

    // --- PERF-2: start IPC server early so the GUI can connect and receive
    // Loading events while slow components (STT, intent) are initialising. ---

    // init IPC broadcast channel
    info!("Initializing IPC...");
    ipc::init();

    // ── Inline validation mode: subprocess called with --audio-test <wav> --validation-out <json>
    // The harness is init'd here (after IPC) so A010 gets a real IPC subscription.
    if let Some(out_path) = testing::replay::parse_inline_validation_output() {
        if let Some(wav_str) = parse_audio_test_arg() {
            testing::harness::TestHarness::init(wav_str, Some(out_path));
        }
    }

    // SEC-7: generate per-session IPC auth token and write to config dir.
    // Must be set BEFORE start_server() so the auth check works from first client.
    let ipc_token = generate_ipc_token();
    if let Some(config_dir) = APP_CONFIG_DIR.get() {
        let token_path = config_dir.join("ipc_token");
        if let Err(e) = std::fs::write(&token_path, &ipc_token) {
            warn!("Failed to write IPC token: {}", e);
        } else {
            // SEC-7: restrict token file to owner-only on Unix (mode 0600)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) = std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600)) {
                    warn!("Failed to set IPC token file permissions: {}", e);
                }
            }
            info!("IPC token written to {:?}", token_path);
        }
    }
    ipc::set_auth_token(ipc_token);

    // channel for text commands (manually written in the GUI)
    let (text_cmd_tx, text_cmd_rx) = mpsc::channel::<String>();

    // runtime handle for async operations inside the sync action handler
    let action_rt = Arc::clone(&rt);

    ipc::set_action_handler(move |action| {
        match action {
            IpcAction::Stop => {
                info!("Received stop command from GUI");
                SHOULD_STOP.store(true, Ordering::SeqCst);
            }
            IpcAction::ReloadCommands => {
                info!("Received reload commands request");
                let rt_clone = Arc::clone(&action_rt);
                std::thread::spawn(move || {
                    match commands::parse_commands() {
                        Ok(new_cmds) => {
                            let result = rt_clone.block_on(intent::reload(&new_cmds));
                            if let Err(e) = result {
                                error!("Intent reload failed: {}", e);
                                ipc::send(IpcEvent::Error {
                                    message: format!("Reload failed: {}", e),
                                });
                                return;
                            }
                            if let Some(lock) = COMMANDS_LIST.get() {
                                *lock.write() = new_cmds;
                                info!("Commands reloaded successfully");
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse commands on reload: {}", e);
                            ipc::send(IpcEvent::Error {
                                message: format!("Reload failed: {}", e),
                            });
                        }
                    }
                });
            }
            IpcAction::SetMuted { muted } => {
                info!("Muted: {}", muted);
                MUTED.store(muted, Ordering::SeqCst);
            }
            IpcAction::TextCommand { text } => {
                info!("Received text command: {}", text);
                if let Err(e) = text_cmd_tx.send(text) {
                    error!("Failed to send text command to app: {}", e);
                }
            }
            IpcAction::Ping => {
                // handled internally by server
            }
            IpcAction::Auth { .. } => {
                // handled in the IPC server before routing to this handler
            }
            IpcAction::ConfirmResult { id, approved } => {
                info!("Confirm result for command '{}': approved={}", id, approved);
                if let Some(pending) = commands::take_pending_command() {
                    if pending.id == id && approved {
                        let id_clone = id.clone();
                        std::thread::spawn(move || {
                            match commands::execute_command(&pending.cmd_path, &pending.cmd, None, None) {
                                Ok(_) => {
                                    info!("[COMMAND] Confirmed command '{}' executed", id_clone);
                                    ipc::send(IpcEvent::CommandExecuted {
                                        id: id_clone.clone(),
                                        success: true,
                                    });
                                }
                                Err(e) => {
                                    error!("[COMMAND] Confirmed command '{}' failed: {}", id_clone, e);
                                    ipc::send(IpcEvent::CommandExecuted {
                                        id: id_clone.clone(),
                                        success: false,
                                    });
                                    ipc::send(IpcEvent::Error { message: e });
                                }
                            }
                            ipc::send(IpcEvent::Idle);
                        });
                    } else {
                        info!("Confirmation denied or ID mismatch for '{}'", id);
                    }
                }
            }
        }
    });

    // start WebSocket server on the shared runtime — do this NOW so GUI can connect
    let ipc_rt = Arc::clone(&rt);
    std::thread::spawn(move || {
        ipc_rt.block_on(ipc::start_server());
    });
    // Give the server a moment to bind before broadcasting Loading events.
    std::thread::sleep(std::time::Duration::from_millis(50));

    // --- Slow initialization with Loading events ---

    // init models registry (scans available AI models)
    if let Err(e) = models::init() {
        warn!("Models registry init failed: {}", e);
    }

    // init stt engine — Vosk model load can take several seconds
    info!("Initializing STT engine...");
    ipc::send(IpcEvent::Loading { component: "stt".to_string() });
    if let Err(e) = stt::init() {
        warn!("STT engine failed to initialize: {}. Voice commands will be unavailable.", e);
        ipc::send(IpcEvent::Error { message: format!("STT unavailable: {}", e) });
    }

    // init commands
    info!("Initializing commands.");
    let cmds = match commands::parse_commands() {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to parse commands: {}. Starting with empty command list.", e);
            Vec::new()
        }
    };
    info!("Commands initialized. Count: {}, List: {:?}", cmds.len(), commands::list_paths(&cmds));

    // SEC-3: warn about Lua commands with unrestricted sandbox access
    let sandbox_full: Vec<String> = cmds
        .iter()
        .flat_map(|cl| cl.commands.iter())
        .filter(|c| c.cmd_type == "lua" && c.sandbox == "full")
        .map(|c| c.id.clone())
        .collect();
    if !sandbox_full.is_empty() {
        warn!("[SEC] Lua commands with sandbox=full (arbitrary shell access): {:?}", sandbox_full);
        // Broadcast to any already-connected clients as well as future ones.
        ipc::send(IpcEvent::SandboxWarning { commands: sandbox_full.clone() });
        ipc::set_sandbox_warnings(sandbox_full);
    }

    COMMANDS_LIST.set(RwLock::new(cmds)).unwrap();

    // init audio
    ipc::send(IpcEvent::Loading { component: "audio".to_string() });
    if audio::init().is_err() {
        // @TODO. Allow continuing even without audio?
        app::close(1); // cannot continue without audio
    }

    // init wake-word engine
    ipc::send(IpcEvent::Loading { component: "listener".to_string() });
    if let Err(e) = listener::init() {
        error!("Wake-word engine init failed: {}", e);
        app::close(1);
    }

    // init intent-recognition engine
    ipc::send(IpcEvent::Loading { component: "intent".to_string() });
    rt.block_on(async {
        let cmds_guard = COMMANDS_LIST.get().unwrap().read();
        if let Err(e) = intent::init(&*cmds_guard).await {
            error!("Failed to initialize intent classifier: {}", e);
            app::close(1);
        }
    });

    // init slots parsing engine
    slots::init().map_err(|e| error!("Slot extraction init failed: {}", e)).ok();

    // init audio processing
    info!("Initializing audio processing...");
    if let Err(e) = audio_processing::init() {
        warn!("Audio processing init failed: {}", e);
    }

    // start the app (in the background thread)
    let app_rt = Arc::clone(&rt);
    std::thread::spawn(move || {
        let _ = app::start(text_cmd_rx, &app_rt);
    });

    // start the central watchdog (after all subsystems are fully initialized)
    watchdog::start();

    // start the production hardening watchdog (slow timescale: 30s/60s/300s)
    runtime_watchdog::start();

    // start the service orchestration runtime (modular service platform)
    orchestration_runtime::start();

    tray::init_blocking(settings);

    Ok(())
}

pub fn should_stop() -> bool {
    SHOULD_STOP.load(Ordering::SeqCst)
}

pub fn is_muted() -> bool {
    MUTED.load(Ordering::SeqCst)
}

fn generate_ipc_token() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn parse_audio_test_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if args[i] == "--audio-test" {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
    }
    std::env::var("JARVIS_AUDIO_TEST_FILE").ok()
}
