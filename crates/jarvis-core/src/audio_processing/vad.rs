mod energy;
mod none;

use once_cell::sync::OnceCell;
use parking_lot::Mutex;

use crate::DB;

static BACKEND: OnceCell<String> = OnceCell::new();

#[cfg(feature = "nnnoiseless")]
static NNNOISELESS_STATE: OnceCell<Mutex<crate::models::nnnoiseless::NnnoiselessVAD>> =
    OnceCell::new();

pub fn init() {
    if BACKEND.get().is_some() {
        return;
    }

    let backend = DB
        .get()
        .map(|db| db.read().vad_backend.clone())
        .unwrap_or_else(|| "energy".to_string());

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
            NNNOISELESS_STATE
                .set(Mutex::new(crate::models::nnnoiseless::NnnoiselessVAD::new()))
                .ok();
            info!("VAD: Nnnoiseless");
        }
        other => {
            warn!("Unknown VAD backend '{}', falling back to energy", other);
            // overwrite with energy
            // (BACKEND already set, so energy::detect will be used via fallthrough)
        }
    }
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
