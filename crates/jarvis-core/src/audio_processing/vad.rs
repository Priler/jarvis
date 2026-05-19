mod none;
mod energy;

use std::sync::atomic::{AtomicU32, Ordering};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;

use crate::{config, DB};

static BACKEND: OnceCell<String> = OnceCell::new();
// Cached energy threshold (f32 bits). 0 means "not yet initialized — use config default".
static ENERGY_THRESHOLD_BITS: AtomicU32 = AtomicU32::new(0);

#[cfg(feature = "nnnoiseless")]
static NNNOISELESS_STATE: OnceCell<Mutex<crate::models::nnnoiseless::NnnoiselessVAD>> = OnceCell::new();

pub fn init() {
    if BACKEND.get().is_some() {
        return;
    }

    let backend = DB.get()
        .map(|db| db.read().vad_backend.clone())
        .unwrap_or_else(|| "energy".to_string());

    // Cache the configurable energy threshold so energy::detect() never reads the DB.
    let threshold = DB.get()
        .map(|db| db.read().vad_energy_threshold)
        .unwrap_or(config::VAD_ENERGY_THRESHOLD);
    ENERGY_THRESHOLD_BITS.store(threshold.to_bits(), Ordering::Relaxed);
    info!("VAD: energy threshold = {:.1}", threshold);

    BACKEND.set(backend.clone()).ok();

    match backend.as_str() {
        "none" => {
            info!("VAD: disabled");
        }
        "energy" => {
            info!("VAD: Energy-based");
        }
        #[cfg(feature = "nnnoiseless")]
        "nnnoiseless" => {
            NNNOISELESS_STATE.set(Mutex::new(crate::models::nnnoiseless::NnnoiselessVAD::new())).ok();
            info!("VAD: Nnnoiseless");
        }
        other => {
            warn!("Unknown VAD backend '{}', falling back to energy", other);
            // overwrite with energy
            // (BACKEND already set, so energy::detect will be used via fallthrough)
        }
    }
}

pub(super) fn energy_threshold() -> f32 {
    let bits = ENERGY_THRESHOLD_BITS.load(Ordering::Relaxed);
    if bits == 0 { config::VAD_ENERGY_THRESHOLD } else { f32::from_bits(bits) }
}

// returns (is_voice, confidence)
pub fn detect(input: &[i16]) -> (bool, f32) {
    match BACKEND.get().map(|s| s.as_str()) {
        Some("none") | None => none::detect(input),
        Some("energy") => energy::detect(input),
        #[cfg(feature = "nnnoiseless")]
        Some("nnnoiseless") => {
            if let Some(state) = NNNOISELESS_STATE.get() {
                state.lock().detect(input)
            } else {
                energy::detect(input)
            }
        }
        _ => energy::detect(input),
    }
}

pub fn reset() {
    match BACKEND.get().map(|s| s.as_str()) {
        #[cfg(feature = "nnnoiseless")]
        Some("nnnoiseless") => {
            if let Some(state) = NNNOISELESS_STATE.get() {
                state.lock().reset();
            }
        }
        _ => {}
    }
}
