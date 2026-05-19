// Simple energy-based VAD — threshold is loaded from DB at startup via vad::init().
pub fn detect(input: &[i16]) -> (bool, f32) {
    let rms = calculate_rms(input);
    let threshold = super::energy_threshold();
    let is_voice = rms > threshold;
    let confidence = (rms / (threshold * 2.0)).min(1.0);
    (is_voice, confidence)
}

fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    
    let sum: f64 = samples.iter()
        .map(|&s| (s as f64).powi(2))
        .sum();
    
    (sum / samples.len() as f64).sqrt() as f32
}