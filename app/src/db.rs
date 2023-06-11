pub mod structs;
use crate::{config, APP_CONFIG_DIR};

use std::path::PathBuf;
use std::fs::File;
use std::io::{BufReader, Read};
use log::info;

use serde_json;

fn get_db_file_path() -> PathBuf {
    PathBuf::from(format!("{}/{}", APP_CONFIG_DIR.get().unwrap().display(), config::DB_FILE_NAME))
}

pub fn init_settings() -> structs::Settings {
    let mut db = None;
    let db_file_path = get_db_file_path();

    info!("Loading settings db file located at: {}", db_file_path.display());

    if db_file_path.exists() {
        // try load existing settings
        if let Ok(mut db_file) = File::open(db_file_path) {
            let reader = BufReader::new(db_file);
            if let Ok(parsed_json) = serde_json::from_reader(reader) {
                info!("Settings loaded.");
                db = Some(parsed_json);
            }
        }
    }

    if db.is_none() {
        // create default settings db file
        warn!("No settings file found or there was an error parsing it. Creating default struct.");
        db = Some(structs::Settings::default());
    }

    db.unwrap()
}

pub fn save_settings(settings: &structs::Settings) -> Result<(), std::io::Error> {
    let db_file_path = get_db_file_path();

    std::fs::write(
        db_file_path,
        serde_json::to_string_pretty(&settings).unwrap()
    )?;

    info!("Settings saved.");
    Ok(())
}