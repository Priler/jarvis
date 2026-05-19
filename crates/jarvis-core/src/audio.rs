mod kira;
mod rodio;

use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::structs::AudioType;
use crate::{config, DB, SOUND_DIR};

static AUDIO_TYPE: OnceCell<AudioType> = OnceCell::new();

/// Timestamp (ms since UNIX epoch) until which the assistant is considered speaking.
static SPEAKING_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

/// Extra silence to add after audio ends so the mic doesn't fire on room reverb.
const SPEAKING_COOLDOWN_MS: u64 = 700;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Returns true while the assistant audio is playing (plus cooldown).
pub fn is_speaking() -> bool {
    SPEAKING_UNTIL_MS.load(Ordering::Acquire) > now_ms()
}

/// Extend the speaking window by `duration` + cooldown.
/// Uses a compare-exchange loop so overlapping sounds accumulate correctly.
fn extend_speaking(duration: std::time::Duration) {
    let duration_ms = duration.as_millis() as u64;
    let until = now_ms() + duration_ms + SPEAKING_COOLDOWN_MS;
    info!(
        "[AUDIO] speaking_until=+{}ms (duration={}ms + cooldown={}ms)",
        duration_ms + SPEAKING_COOLDOWN_MS,
        duration_ms,
        SPEAKING_COOLDOWN_MS
    );
    let mut cur = SPEAKING_UNTIL_MS.load(Ordering::Acquire);
    loop {
        if until <= cur {
            break;
        }
        match SPEAKING_UNTIL_MS.compare_exchange_weak(cur, until, Ordering::Release, Ordering::Acquire) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
}

pub fn init() -> Result<(), ()> {
    if AUDIO_TYPE.get().is_some() {
        return Ok(());
    } // already initialized

    // set default audio type
    // @TODO. Make it configurable?
    AUDIO_TYPE.set(config::DEFAULT_AUDIO_TYPE).unwrap();

    // load given audio backend
    match AUDIO_TYPE.get().unwrap() {
        AudioType::Rodio => {
            // Init Rodio
            info!("Initializing Rodio audio backend.");

            match rodio::init() {
                Ok(_) => {
                    info!("Successfully initialized Rodio audio backend.");
                }
                Err(()) => {
                    error!("Failed to initialize Rodio audio backend.");

                    return Err(());
                }
            }
        }
        AudioType::Kira => {
            // Init Kira
            info!("Initializing Kira audio backend.");

            match kira::init() {
                Ok(_) => {
                    info!("Successfully initialized Kira audio backend.");
                }
                Err(_msg) => {
                    error!("Failed to initialize Kira audio backend.");

                    return Err(());
                }
            }
        }
    }

    Ok(())
}

pub fn play_sound(filename: &PathBuf) {
    let audio_type = match AUDIO_TYPE.get() {
        Some(t) => t,
        None => {
            warn!("Audio not initialized, cannot play: {}", filename.display());
            return;
        }
    };
    
    info!("[AUDIO] play started file={}", filename.display());

    let duration = match audio_type {
        AudioType::Rodio => {
            rodio::play_sound(filename, true);
            std::time::Duration::ZERO
        }
        AudioType::Kira => kira::play_sound(filename),
    };

    info!("[AUDIO] duration_ms={}", duration.as_millis());
    extend_speaking(duration);
}

pub fn get_sound_directory() -> Option<PathBuf> {
    let db = DB.get()?;

    let voice_path = {
        let s = db.read();
        SOUND_DIR.join(&s.voice)
    };

    match voice_path.exists() {
        true => Some(voice_path),
        _ => {
            error!("No sounds folder found. Search path - {:?}", voice_path);
            None
        }
    }
}
