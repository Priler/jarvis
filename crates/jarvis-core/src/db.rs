pub mod structs;
pub mod manager;

use crate::{config, APP_CONFIG_DIR};

use log::info;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

pub use manager::SettingsManager;

fn get_db_file_path() -> PathBuf {
    PathBuf::from(format!(
        "{}/{}",
        APP_CONFIG_DIR.get().unwrap().display(),
        config::DB_FILE_NAME
    ))
}

pub fn init_settings() -> structs::Settings {
    let db_file_path = get_db_file_path();

    info!(
        "Loading settings db file located at: {}",
        db_file_path.display()
    );

    let mut settings = if db_file_path.exists() {
        if let Ok(db_file) = File::open(&db_file_path) {
            let reader = BufReader::new(db_file);
            if let Ok(s) = serde_json::from_reader(reader) {
                info!("Settings loaded.");
                s
            } else {
                warn!("Error parsing settings file. Creating default struct.");
                structs::Settings::default()
            }
        } else {
            warn!("Cannot open settings file. Creating default struct.");
            structs::Settings::default()
        }
    } else {
        warn!("No settings file found. Creating default struct.");
        structs::Settings::default()
    };

    // SEC-6: migrate Picovoice API key from plaintext JSON to OS keyring.
    // The field is deserialized from old JSON files but never written back.
    if !settings.api_keys.picovoice.is_empty() {
        let old_key = std::mem::take(&mut settings.api_keys.picovoice);
        match crate::keychain::set_api_key("picovoice", &old_key) {
            Ok(()) => {
                info!("Migrated Picovoice API key from settings file to OS keyring.");
                // Overwrite settings file immediately so the plaintext key is removed.
                if let Err(e) = save_settings(&settings) {
                    warn!("Failed to overwrite settings file after key migration: {}", e);
                }
            }
            Err(e) => {
                warn!("Failed to migrate Picovoice API key to keyring: {}. Key left in memory only.", e);
                settings.api_keys.picovoice = old_key;
            }
        }
    }

    settings
}

/// init settings and return a SettingsManager ready to use
pub fn init() -> SettingsManager {
    let settings = init_settings();
    SettingsManager::new(settings)
}

pub fn save_settings(settings: &structs::Settings) -> Result<(), std::io::Error> {
    let db_file_path = get_db_file_path();

    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Write to a temp file then rename atomically to avoid partial writes corrupting settings.
    let tmp_path = db_file_path.with_extension("tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &db_file_path)?;

    info!("Settings saved to: {:#}", db_file_path.display());
    Ok(())
}
