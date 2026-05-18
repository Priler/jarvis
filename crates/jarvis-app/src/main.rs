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

// include log
#[macro_use]
extern crate simple_log;
mod log;

// include app
mod app;

// include tray
// @TODO. macOS currently not supported for tray functionality.
#[cfg(not(target_os = "macos"))]
mod tray;

static SHOULD_STOP: AtomicBool = AtomicBool::new(false);

fn main() -> Result<(), String> {
    // initialize directories
    config::init_dirs()?;

    // initialize logging
    log::init_logging()?;

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
        info!("[AUDIO_TEST] WAV test mode enabled: {}", path);
        if let Err(e) = recorder::init_wav(path) {
            error!("[AUDIO_TEST] Failed to initialize WAV source: {}", e);
            app::close(1);
        }
    } else if recorder::init().is_err() {
        app::close(1);
    }

    // init models registry (scans available AI models)
    if let Err(e) = models::init() {
        warn!("Models registry init failed: {}", e);
    }

    // init stt engine
    if stt::init().is_err() {
        // @TODO. Allow continuing even without STT, if commands is using keywords or smthng?
        app::close(1); // cannot continue without stt
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
        ipc::set_sandbox_warnings(sandbox_full);
    }

    COMMANDS_LIST.set(cmds).unwrap();

    // init audio
    if audio::init().is_err() {
        // @TODO. Allow continuing even without audio?
        app::close(1); // cannot continue without audio
    }

    // init wake-word engine
    if let Err(e) = listener::init() {
        error!("Wake-word engine init failed: {}", e);
        app::close(1);
    }

    // shared async runtime for intent classification, IPC, etc.
    let rt = Arc::new(
        tokio::runtime::Runtime::new().expect("Failed to create tokio runtime")
    );

    // init intent-recognition engine
    rt.block_on(async {
        if let Err(e) = intent::init(COMMANDS_LIST.get().unwrap()).await {
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

    // init IPC
    info!("Initializing IPC...");
    ipc::init();

    // SEC-7: generate per-session IPC auth token and write to config dir
    let ipc_token = generate_ipc_token();
    if let Some(config_dir) = APP_CONFIG_DIR.get() {
        let token_path = config_dir.join("ipc_token");
        if let Err(e) = std::fs::write(&token_path, &ipc_token) {
            warn!("Failed to write IPC token: {}", e);
        } else {
            info!("IPC token written to {:?}", token_path);
        }
    }
    ipc::set_auth_token(ipc_token);

    // channel for text commands (manually written in the GUI)
    let (text_cmd_tx, text_cmd_rx) = mpsc::channel::<String>();

    ipc::set_action_handler(move |action| {
        match action {
            IpcAction::Stop => {
                info!("Received stop command from GUI");
                SHOULD_STOP.store(true, Ordering::SeqCst);
            }
            IpcAction::ReloadCommands => {
                info!("Received reload commands request");
                // TODO: implement reload
            }
            IpcAction::SetMuted { muted } => {
                info!("Received mute request: {}", muted);
                // TODO: implement mute
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

    // start WebSocket server on the shared runtime
    let ipc_rt = Arc::clone(&rt);
    std::thread::spawn(move || {
        ipc_rt.block_on(ipc::start_server());
    });
    
    // start the app (in the background thread)
    let app_rt = Arc::clone(&rt);
    std::thread::spawn(move || {
        let _ = app::start(text_cmd_rx, &app_rt);
    });

    tray::init_blocking(settings);

    Ok(())
}

pub fn should_stop() -> bool {
    SHOULD_STOP.load(Ordering::SeqCst)
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
