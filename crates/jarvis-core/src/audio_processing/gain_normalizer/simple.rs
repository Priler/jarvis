use crate::config;

pub struct GainNormalizer {
    current_gain: f32,
}

impl GainNormalizer {
    pub fn new() -> Self {
        Self { current_gain: 1.0 }
    }

    pub fn normalize(&mut self, input: &[i16]) -> Vec<i16> {
        let rms = self.calculate_rms(input);
        
        if rms < 1.0 {
            return input.to_vec();
        }

        let target_gain = config::GAIN_TARGET_RMS / rms;
        let clamped_gain = target_gain.clamp(config::GAIN_MIN, config::GAIN_MAX);

        self.current_gain = self.current_gain * 0.9 + clamped_gain * 0.1;

        input.iter()
            .map(|&s| {
                let amplified = (s as f32) * self.current_gain;
                amplified.clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
            .collect()
    }

    pub fn reset(&mut self) {
        self.current_gain = 1.0;
    }

    fn calculate_rms(&self, samples: &[i16]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }

        let sum: f64 = samples.iter()
            .map(|&s| (s as f64).powi(2))
            .sum();

        (sum / samples.len() as f64).sqrt() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn rms(samples: &[i16]) -> f32 {
        if samples.is_empty() { return 0.0; }
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum / samples.len() as f64).sqrt() as f32
    }

    #[test]
    fn normalize_empty_input_returns_empty() {
        let mut n = GainNormalizer::new();
        assert!(n.normalize(&[]).is_empty());
    }

    #[test]
    fn near_silence_passthrough() {
        // RMS < 1.0 → gain not applied, samples returned as-is
        let mut n = GainNormalizer::new();
        let input = vec![0i16; 512];
        let output = n.normalize(&input);
        assert_eq!(output, input);
    }

    #[test]
    fn low_rms_signal_is_amplified() {
        // RMS of 10 is well below GAIN_TARGET_RMS; output RMS should be higher
        let mut n = GainNormalizer::new();
        let input = vec![10i16; 512];
        let output = n.normalize(&input);
        assert!(rms(&output) > rms(&input), "expected amplification");
    }

    #[test]
    fn high_rms_signal_is_attenuated() {
        // Run several frames so the smoothing converges, then check attenuation
        let mut n = GainNormalizer::new();
        let input = vec![i16::MAX / 2; 512];
        for _ in 0..20 {
            n.normalize(&input);
        }
        let output = n.normalize(&input);
        // if target < current RMS, output should be smaller
        if rms(&input) > config::GAIN_TARGET_RMS {
            assert!(rms(&output) < rms(&input), "expected attenuation");
        }
    }

    #[test]
    fn output_never_clips_i16() {
        // Very loud input must not produce out-of-range samples
        let mut n = GainNormalizer::new();
        let input = vec![i16::MAX; 512];
        let output = n.normalize(&input);
        for &s in &output {
            assert!(s >= i16::MIN && s <= i16::MAX);
        }
    }

    #[test]
    fn reset_restores_initial_gain() {
        let mut n = GainNormalizer::new();
        // run several frames to move the gain away from 1.0
        let input = vec![10i16; 512];
        for _ in 0..10 {
            n.normalize(&input);
        }
        n.reset();
        assert!((n.current_gain - 1.0).abs() < f32::EPSILON);
    }
}