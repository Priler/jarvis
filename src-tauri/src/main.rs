// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
extern crate lazy_static; // better switch to once_cell ?
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};
use log::{info};
use log::LevelFilter;
use std::sync::Mutex;

// expose the config
mod config;
use config::*;

// include tauri commands
mod tauri_commands;

// include assistant commands
mod assistant_commands;
use assistant_commands::AssistantCommand;

// include vosk
mod vosk;

// include events
mod events;

// include recorder
mod recorder;

// app dir
lazy_static! {
    static ref APP_CONFIG_DIR: Mutex<String> = Mutex::new(String::new());
}

// init PickleDb connection
lazy_static! {
    static ref DB: Mutex<PickleDb> = Mutex::new(
        PickleDb::load(
            format!("{}/{}", APP_CONFIG_DIR.lock().unwrap(), DB_FILE_NAME),
            PickleDbDumpPolicy::AutoDump,
            SerializationMethod::Json
        )
        .unwrap_or_else(|_x: _| {
            info!("Creating new db file at {} ...", format!("{}/{}", APP_CONFIG_DIR.lock().unwrap(), DB_FILE_NAME));
            PickleDb::new(
                format!("{}/{}", APP_CONFIG_DIR.lock().unwrap(), DB_FILE_NAME),
                PickleDbDumpPolicy::AutoDump,
                SerializationMethod::Json,
            )
        })
    );
}

// init commands
lazy_static! {
    static ref COMMANDS: Vec<AssistantCommand> = assistant_commands::parse_commands().unwrap();
}

fn main() {
    // log to file
    simple_logging::log_to_file(config::LOG_FILE_NAME, LevelFilter::max()).expect("Failed to start logger ... is directory writable?");

    // init vosk
    vosk::init_vosk();

    // run the app
    tauri::Builder::default()
        .setup(|app| {
            std::fs::create_dir_all(app.path_resolver().app_config_dir().unwrap())?;
            APP_CONFIG_DIR.lock().unwrap().push_str(app.path_resolver().app_config_dir().unwrap().to_str().unwrap());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // db commands
            tauri_commands::db_read,
            tauri_commands::db_write,
            // recorder commands
            tauri_commands::pv_get_audio_devices,
            tauri_commands::pv_get_audio_device_name,
            // listener commands
            tauri_commands::start_listening,
            tauri_commands::stop_listening,
            tauri_commands::is_listening,
            // sys commands
            tauri_commands::get_current_ram_usage,
            tauri_commands::get_peak_ram_usage,
            tauri_commands::get_cpu_temp,
            tauri_commands::get_cpu_usage,
            // sound commands
            tauri_commands::play_sound,
            // etc commands
            tauri_commands::get_app_version,
            tauri_commands::get_author_name,
            tauri_commands::get_repository_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
