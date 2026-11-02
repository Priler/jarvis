use once_cell::sync::{Lazy, OnceCell};
use parking_lot::RwLock;
use platform_dirs::AppDirs;
use std::path::PathBuf;
use std::sync::Arc;

#[macro_use]
extern crate log;

pub mod time;

pub mod audio;
pub mod commands;
pub mod config;
pub mod db;
pub mod i18n;

#[cfg(feature = "jarvis_app")]
pub mod listener;

pub mod recorder;

#[cfg(feature = "jarvis_app")]
pub mod stt;

#[cfg(feature = "intent")]
pub mod intent;

#[cfg(feature = "jarvis_app")]
pub mod slots;

pub mod models;

// re-exported from models/
pub use models::gliner_models;
pub use models::vosk_models;

#[cfg(feature = "jarvis_app")]
pub mod audio_processing;

#[cfg(feature = "jarvis_app")]
pub mod ipc;

pub mod voices;

pub mod audio_buffer;

#[cfg(feature = "lua")]
pub mod lua;

// shared statics
fn has_resources(path: &std::path::Path) -> bool {
    path.join("resources").is_dir()
}

pub static APP_DIR: Lazy<PathBuf> = Lazy::new(|| {
    if let Ok(path) = std::env::var("JARVIS_APP_DIR") {
        let path = PathBuf::from(path);
        if has_resources(&path) {
            return path;
        }
        warn!(
            "JARVIS_APP_DIR does not contain resources: {}",
            path.display()
        );
    }

    if let Ok(path) = std::env::current_dir() {
        if has_resources(&path) {
            return path;
        }
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    if let Some(path) = exe_dir.as_ref() {
        if has_resources(path) {
            return path.clone();
        }
    }

    exe_dir.unwrap_or_else(|| std::env::current_dir().unwrap())
});
pub static SOUND_DIR: Lazy<PathBuf> = Lazy::new(|| APP_DIR.clone().join(config::SOUND_PATH));
pub static APP_DIRS: OnceCell<AppDirs> = OnceCell::new();
pub static APP_CONFIG_DIR: OnceCell<PathBuf> = OnceCell::new();
pub static APP_LOG_DIR: OnceCell<PathBuf> = OnceCell::new();
pub static DB: OnceCell<Arc<RwLock<db::structs::Settings>>> = OnceCell::new();
pub static COMMANDS_LIST: OnceCell<Vec<JCommandsList>> = OnceCell::new();

// re-exports
pub use commands::JCommandsList;
pub use config::structs::*;
pub use db::structs::Settings;
pub use db::SettingsManager;

// use crate::commands::{JComandsList, JCommand};
