mod porcupine;

mod rustpotter;

mod vosk;

use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{config, stt};
use crate::config::structs::WakeWordEngine;

use crate::DB;

// store wake-word engine being used
static WAKE_WORD_ENGINE: OnceCell<WakeWordEngine> = OnceCell::new();

// track listening state
static LISTENING: AtomicBool = AtomicBool::new(false);

pub fn init() -> Result<(), ()> {
    if !WAKE_WORD_ENGINE.get().is_none() {return Ok(());} // already initialized

    // store current engine
    WAKE_WORD_ENGINE.set(DB.get().unwrap().wake_word_engine).unwrap();

    // load given wake-word engine
    match WAKE_WORD_ENGINE.get().unwrap() {
        WakeWordEngine::Porcupine => {
            // Init Porcupine wake-word engine
            info!("Initializing Porcupine wake-word engine.");

            return porcupine::init();
        },
        WakeWordEngine::Rustpotter => {
            // Init Rustpotter wake-word engine
            info!("Initializing Rustpotter wake-word engine.");

            return rustpotter::init();
        },
        WakeWordEngine::Vosk => {
            // Init Vosk as wake-word engine (very slow, though)
            info!("Initializing Vosk as wake-word engine.");
            warn!("Using Vosk as wake-word engine is highly not recommended, because it's very slow for this task.");

            return vosk::init();
        },
    }
}

pub fn data_callback(frame_buffer: &[i16]) -> Option<i32> {
    match WAKE_WORD_ENGINE.get().unwrap() {
        WakeWordEngine::Porcupine => {
            porcupine::data_callback(frame_buffer)
        },
        WakeWordEngine::Rustpotter => {
            rustpotter::data_callback(frame_buffer)
        },
        WakeWordEngine::Vosk => {
            vosk::data_callback(frame_buffer)
        }
    }
}