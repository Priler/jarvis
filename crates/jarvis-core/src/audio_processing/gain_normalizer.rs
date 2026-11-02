mod simple;

use once_cell::sync::OnceCell;
use parking_lot::Mutex;

static NORMALIZER: OnceCell<Mutex<simple::GainNormalizer>> = OnceCell::new();

pub fn init() {
    if NORMALIZER.get().is_some() {
        return;
    }

    NORMALIZER
        .set(Mutex::new(simple::GainNormalizer::new()))
        .ok();
    info!("Gain normalizer: enabled");
}

pub fn normalize(input: &[i16]) -> Vec<i16> {
    match NORMALIZER.get() {
        Some(n) => n.lock().normalize(input),
        None => input.to_vec(),
    }
}

pub fn reset() {
    if let Some(n) = NORMALIZER.get() {
        n.lock().reset();
    }
}
