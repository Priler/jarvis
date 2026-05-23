// import AUDIO commands
mod audio;
pub use audio::*;

// import DB related commands
mod db;
pub use db::*;

// import LISTENER commands
// @REMOVED: gui not listens anymore
// mod listener;
// pub use listener::*;

// import ETC commands
mod etc;
pub use etc::*;

// import FS commands
mod fs;
pub use fs::*;

// import SYS commands
mod sys;
pub use sys::*;

// import STT commands
mod stt;
pub use stt::*;

// import i18n commands
mod i18n;
pub use i18n::*;

// import commands commands xD
mod commands;
pub use commands::*;

// import voices commands
mod voices;
pub use voices::*;

// import ollama commands
mod ollama;
pub use ollama::*;