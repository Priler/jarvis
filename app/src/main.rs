use std::env;
use std::error::Error;
use std::path::PathBuf;

use once_cell::sync::{Lazy, OnceCell};
use platform_dirs::{AppDirs};

// expose the config
mod config;

// include log
#[macro_use]
extern crate simple_log;
mod log;

// include app
mod app;

// include db
mod db;

// include tray
// @TODO. macOS currently not supported for tray functionality.
#[cfg(not(target_os = "macos"))]
mod tray;

// include recorder
mod recorder;

// include speech-to-text
mod stt;

// include text-to-speech
// empty

// include commands
mod commands;
use commands::AssistantCommand;
use crate::commands::list;

// include audio
mod audio;

// include listener
mod listener;

// some global data
static APP_DIR: Lazy<PathBuf> = Lazy::new(|| {env::current_dir().unwrap()});
static SOUND_DIR: Lazy<PathBuf> = Lazy::new(|| {APP_DIR.clone().join("sound")});
static APP_DIRS: OnceCell<AppDirs> = OnceCell::new();
static APP_CONFIG_DIR: OnceCell<PathBuf> = OnceCell::new();
static APP_LOG_DIR: OnceCell<PathBuf> = OnceCell::new();
static DB: OnceCell<db::structs::Settings> = OnceCell::new();
static COMMANDS_LIST: OnceCell<Vec<AssistantCommand>> = OnceCell::new();

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

    // initialize tray
    // @TODO. macOS currently not supported for tray functionality,
    // due to the separate thread in which tray processing works,
    // but macOS requires it to be processed in the main thread only
    // The solution may be to include wake-word detection etc. in the winit event loop. (only for MacOS, though?)
    #[cfg(not(target_os = "macos"))]
    tray::init();

    // init recorder
    if recorder::init().is_err() {
        app::close(1); // cannot continue without recorder
    }

    // init stt engine
    if stt::init().is_err() {
        // @TODO. Allow continuing even without STT, if commands is using keywords or smthng?
        app::close(1); // cannot continue without stt
    }

    // init tts engine
    // none for now (Silero-rs coming)

    // init commands
    info!("Initializing commands.");
    let commands = commands::parse_commands().unwrap();
    info!("Commands initialized.\nOverall commands parsed: {}\nParsed commands: {:?}", commands.len(), commands::list(&commands));
    COMMANDS_LIST.set(commands).unwrap();

    // init audio
    if audio::init().is_err() {
        // @TODO. Allow continuing even without audio?
        app::close(1); // cannot continue without audio
    }

    // init wake-word engine
    if listener::init().is_err() {
        app::close(1); // cannot continue without wake-word engine
    }

    // start the app
    app::start();

    Ok(())
}