use crate::config;

// Simple energy-based VAD
pub fn detect(input: &[i16]) -> (bool, f32) {
    let rms = calculate_rms(input);
    let is_voice = rms > config::VAD_ENERGY_THRESHOLD;

    // normalize confidence to 0-1 range (rough approximation)
    let confidence = (rms / (config::VAD_ENERGY_THRESHOLD * 2.0)).min(1.0);

    (is_voice, confidence)
}

fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();

    (sum / samples.len() as f64).sqrt() as f32
}
