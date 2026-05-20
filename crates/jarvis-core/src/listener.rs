mod rustpotter;
mod vosk;

use once_cell::sync::OnceCell;

use crate::config::structs::WakeWordEngine;

use crate::DB;

static WAKE_WORD_ENGINE: OnceCell<WakeWordEngine> = OnceCell::new();

pub fn init() -> Result<(), String> {
    if WAKE_WORD_ENGINE.get().is_some() {
        return Ok(());
    }

    let engine = DB.get().unwrap().read().wake_word_engine;

    WAKE_WORD_ENGINE.set(engine)
        .map_err(|_| "Wake word engine already set".to_string())?;

    match engine {
        WakeWordEngine::Porcupine => {
            Err("Porcupine wake-word engine is not supported".to_string())
        }
        WakeWordEngine::Rustpotter => {
            info!("Initializing Rustpotter wake-word engine.");
            rustpotter::init()
                .map_err(|_| "Failed to init Rustpotter".to_string())
        }
        WakeWordEngine::Vosk => {
            info!("Initializing Vosk as wake-word engine.");
            warn!("Using Vosk as wake-word engine is highly not recommended, because it's very slow for this task.");
            vosk::init()
                .map_err(|_| "Failed to init Vosk wake-word".to_string())
        }
    }
}

pub fn data_callback(frame_buffer: &[i16]) -> Option<i32> {
    match WAKE_WORD_ENGINE.get()? {
        WakeWordEngine::Porcupine => None,
        WakeWordEngine::Rustpotter => rustpotter::data_callback(frame_buffer),
        WakeWordEngine::Vosk => vosk::data_callback(frame_buffer),
    }
}

pub fn get_max_score() -> f32 {
    match WAKE_WORD_ENGINE.get() {
        Some(WakeWordEngine::Rustpotter) => rustpotter::get_max_score(),
        _ => 0.0,
    }
}

pub fn get_frame_count() -> usize {
    match WAKE_WORD_ENGINE.get() {
        Some(WakeWordEngine::Rustpotter) => rustpotter::get_frame_count(),
        _ => 0,
    }
}

/// Reset all internal engine state accumulated since the last wake session.
/// For Rustpotter this clears the rechunking remainder buffer; other engines
/// are no-ops unless they accumulate internal state.
pub fn reset_state() {
    match WAKE_WORD_ENGINE.get() {
        Some(WakeWordEngine::Rustpotter) => rustpotter::reset_remainder(),
        _ => {}
    }
}
