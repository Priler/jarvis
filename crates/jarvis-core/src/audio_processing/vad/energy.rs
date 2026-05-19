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

#[cfg(test)]
mod tests {
    use super::*;

    // Default threshold is config::VAD_ENERGY_THRESHOLD (100.0) when ENERGY_THRESHOLD_BITS == 0.

    #[test]
    fn silent_frame_is_not_voice() {
        let frame = vec![0i16; 512];
        let (is_voice, confidence) = detect(&frame);
        assert!(!is_voice);
        assert_eq!(confidence, 0.0);
    }

    #[test]
    fn loud_frame_is_voice() {
        // RMS of constant i16::MAX ≈ 32767, well above threshold 100
        let frame = vec![i16::MAX; 512];
        let (is_voice, _) = detect(&frame);
        assert!(is_voice);
    }

    #[test]
    fn just_below_threshold_is_not_voice() {
        // RMS of constant value v equals |v|. threshold = 100.0; v=100 → RMS=100 (not > threshold)
        let frame = vec![100i16; 512];
        let (is_voice, _) = detect(&frame);
        assert!(!is_voice);
    }

    #[test]
    fn just_above_threshold_is_voice() {
        let frame = vec![101i16; 512];
        let (is_voice, _) = detect(&frame);
        assert!(is_voice);
    }

    #[test]
    fn confidence_clamped_to_one_for_very_loud_signal() {
        let frame = vec![i16::MAX; 512];
        let (_, confidence) = detect(&frame);
        assert!(confidence <= 1.0);
    }

    #[test]
    fn empty_frame_is_not_voice() {
        let (is_voice, confidence) = detect(&[]);
        assert!(!is_voice);
        assert_eq!(confidence, 0.0);
    }

    #[test]
    fn rms_matches_constant_signal() {
        // RMS of constant signal v should equal |v|
        let v = 500i16;
        let frame = vec![v; 256];
        let rms = calculate_rms(&frame);
        assert!((rms - v as f32).abs() < 1.0, "rms={rms}, expected={v}");
    }

    #[test]
    fn rms_of_alternating_signs_is_same_as_constant() {
        // RMS is unaffected by sign (squares both ways)
        let pos: Vec<i16> = (0..256).map(|_| 500).collect();
        let alt: Vec<i16> = (0..256).map(|i| if i % 2 == 0 { 500 } else { -500 }).collect();
        let rms_pos = calculate_rms(&pos);
        let rms_alt = calculate_rms(&alt);
        assert!((rms_pos - rms_alt).abs() < 1.0);
    }
}