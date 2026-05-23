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

/// Reference counter: incremented when a sound starts, decremented after estimated
/// playback + cooldown elapses.  `is_speaking()` checks both the timestamp and this
/// counter so the gate stays open for the full playback duration even for backends
/// (Rodio) that do not return the actual sound duration.
pub static ACTIVE_PLAYBACK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Bumped by `force_clear_speaking` to invalidate in-flight decrement timers.
static PLAYBACK_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Extra silence to add after audio ends so the mic doesn't fire on room reverb.
const SPEAKING_COOLDOWN_MS: u64 = 700;

/// Fallback playback duration for Rodio (which does not return the actual duration).
const RODIO_ESTIMATED_DURATION_MS: u64 = 2_000;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Returns true while the assistant audio is playing (plus cooldown).
/// Checks both the deadline timestamp AND the active playback reference count so
/// the gate stays closed for the full sound even when the backend (Rodio) cannot
/// supply the exact duration.
pub fn is_speaking() -> bool {
    SPEAKING_UNTIL_MS.load(Ordering::Acquire) > now_ms()
        || ACTIVE_PLAYBACK_COUNT.load(Ordering::Relaxed) > 0
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

    // Reference-counting: increment before playing; a background timer decrements
    // after the estimated playback + cooldown window expires.
    ACTIVE_PLAYBACK_COUNT.fetch_add(1, Ordering::AcqRel);
    let my_gen = PLAYBACK_GENERATION.load(Ordering::Acquire);

    let duration = match audio_type {
        AudioType::Rodio => {
            rodio::play_sound(filename, true);
            std::time::Duration::ZERO
        }
        AudioType::Kira => kira::play_sound(filename),
    };

    // For Rodio (duration==ZERO) use a fixed estimate; for Kira use the real duration.
    let decrement_after_ms = if duration == std::time::Duration::ZERO {
        RODIO_ESTIMATED_DURATION_MS + SPEAKING_COOLDOWN_MS
    } else {
        duration.as_millis() as u64 + SPEAKING_COOLDOWN_MS
    };

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(decrement_after_ms));
        // Skip decrement if force_clear_speaking() was called since we incremented.
        if PLAYBACK_GENERATION.load(Ordering::Acquire) == my_gen
            && ACTIVE_PLAYBACK_COUNT.load(Ordering::Acquire) > 0
        {
            ACTIVE_PLAYBACK_COUNT.fetch_sub(1, Ordering::Release);
        }
    });

    info!("[AUDIO] duration_ms={}", duration.as_millis());
    extend_speaking(duration);
}

/// Force-clear the speaking gate.  Resets both the timestamp and the reference
/// count, and bumps the generation so any in-flight decrement timers become no-ops.
pub fn force_clear_speaking() {
    let prev = SPEAKING_UNTIL_MS.swap(0, Ordering::Release);
    ACTIVE_PLAYBACK_COUNT.store(0, Ordering::Release);
    PLAYBACK_GENERATION.fetch_add(1, Ordering::AcqRel);
    warn!(
        "[AUDIO] Speaking gate force-cleared prev_until_ms={}",
        prev
    );
}

/// Returns how many milliseconds remain on the speaking gate. 0 if not active.
pub fn speaking_remaining_ms() -> u64 {
    SPEAKING_UNTIL_MS.load(Ordering::Acquire).saturating_sub(now_ms())
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
