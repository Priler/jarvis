

#[macro_use]
extern crate lazy_static; // better switch to once_cell ?
use pickledb::{PickleDb, PickleDbDumpPolicy, SerializationMethod};
use log::{info};
use log::LevelFilter;
use std::sync::Mutex;

// include assistant commands
mod assistant_commands;
use assistant_commands::AssistantCommand;

// include vosk
mod vosk;

// include events
mod events;

// include recorder
mod recorder;

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

fn main() {
    // init vosk
    vosk::init_vosk();

    // run the app
    tauri::Builder::default()
        .setup(|app| {
            std::fs::create_dir_all(app.path_resolver().app_config_dir().unwrap())?;
            APP_CONFIG_DIR.lock().unwrap().push_str(app.path_resolver().app_config_dir().unwrap().to_str().unwrap());

            std::fs::create_dir_all(app.path_resolver().app_log_dir().unwrap())?;
            APP_LOG_DIR.lock().unwrap().push_str(app.path_resolver().app_log_dir().unwrap().to_str().unwrap());

            // log to file
            let log_file_path = format!("{}/{}", APP_LOG_DIR.lock().unwrap(), config::LOG_FILE_NAME);
            println!("!!!===============!!!\nLOGGING TO {}\n!!!===============!!!\n", &log_file_path);
            simple_logging::log_to_file(&log_file_path, LevelFilter::max()).expect("Failed to start logger ... is directory writable?");

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
            // fs commands
            tauri_commands::show_in_folder,
            // etc commands
            tauri_commands::get_app_version,
            tauri_commands::get_author_name,
            tauri_commands::get_repository_link,
            tauri_commands::get_tg_official_link,
            tauri_commands::get_feedback_link,
            tauri_commands::get_log_file_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
