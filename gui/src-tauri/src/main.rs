// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use once_cell::sync::OnceCell;
use platform_dirs::{AppDirs};

// expose the config
mod config;

// include log
#[macro_use]
extern crate simple_log;
mod log;

// include db
mod db;

// include tray
mod tray;

// include tauri commands
// mod tauri_commands;

// some global data
static APP_DIRS: OnceCell<AppDirs> = OnceCell::new();
static APP_CONFIG_DIR: OnceCell<PathBuf> = OnceCell::new();
static APP_LOG_DIR: OnceCell<PathBuf> = OnceCell::new();
static DB: OnceCell<db::structs::Settings> = OnceCell::new();

fn main() -> Result<(), String> {
    // initialize directories
    config::init_dirs()?;

    // initialize logging
    log::init_logging()?;

    // log some base info
    info!("Starting Jarvis v{} ...", config::APP_VERSION.unwrap());
    info!("Config directory is: {}", APP_CONFIG_DIR.get().unwrap().display());
    info!("Log directory is: {}", APP_LOG_DIR.get().unwrap().display());

    // initialize database (settings)
    DB.set(db::init_settings());

    Ok(())
}