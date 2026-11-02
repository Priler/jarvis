mod none;

use once_cell::sync::OnceCell;
use parking_lot::Mutex;

use crate::config::structs::NoiseSuppressionBackend;

static BACKEND: OnceCell<NoiseSuppressionBackend> = OnceCell::new();

#[cfg(feature = "nnnoiseless")]
static NNNOISELESS_STATE: OnceCell<Mutex<crate::models::nnnoiseless::NnnoiselessNS>> =
    OnceCell::new();

pub fn init(backend: NoiseSuppressionBackend) {
    if BACKEND.get().is_some() {
        return;
    }

    // fallback if nnnoiseless not compiled in
    #[cfg(not(feature = "nnnoiseless"))]
    if matches!(backend, NoiseSuppressionBackend::Nnnoiseless) {
        warn!("Nnnoiseless not compiled in, falling back to None");
        backend = NoiseSuppressionBackend::None;
    }

    BACKEND.set(backend).ok();

    match backend {
        NoiseSuppressionBackend::None => {
            info!("Noise suppression: disabled");
        }
        #[cfg(feature = "nnnoiseless")]
        NoiseSuppressionBackend::Nnnoiseless => {
            NNNOISELESS_STATE
                .set(Mutex::new(crate::models::nnnoiseless::NnnoiselessNS::new()))
                .ok();
            info!("Noise suppression: Nnnoiseless");
        }
        #[cfg(not(feature = "nnnoiseless"))]
        _ => {}
    }
}

pub fn process(input: &[i16]) -> Vec<i16> {
    match BACKEND.get() {
        #[cfg(feature = "nnnoiseless")]
        Some(NoiseSuppressionBackend::Nnnoiseless) => {
            if let Some(state) = NNNOISELESS_STATE.get() {
                state.lock().process(input)
            } else {
                none::process(input)
            }
        }
        _ => none::process(input),
    }
}

pub fn reset() {
    match BACKEND.get() {
        #[cfg(feature = "nnnoiseless")]
        Some(NoiseSuppressionBackend::Nnnoiseless) => {
            if let Some(state) = NNNOISELESS_STATE.get() {
                state.lock().reset();
            }
        }
        _ => {}
    }
}
