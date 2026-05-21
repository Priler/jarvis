//! TTS self-hearing fingerprint for playback suppression.
//!
//! Computes a lightweight spectral fingerprint from PCM samples and uses it
//! to detect when a detected "wake" is actually the assistant's own TTS voice
//! reflected back through the microphone.
//!
//! No external audio-analysis crates — everything is computed from raw i16 PCM.

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::environment_profile::RuntimeMode;

// ── Fingerprint ───────────────────────────────────────────────────────────────

/// Lightweight spectral shape descriptor derived from a PCM frame.
#[derive(Clone, Debug, Default)]
pub struct VoiceFingerprint {
    /// Spectral centroid normalised to [0, 1] relative to Nyquist.
    pub spectral_centroid: f32,
    /// Normalised energy in 0–300 Hz band.
    pub energy_low: f32,
    /// Normalised energy in 300–2000 Hz band.
    pub energy_mid: f32,
    /// Normalised energy in 2000+ Hz band.
    pub energy_high: f32,
    /// Fraction of samples with |amplitude| above voiced-energy threshold.
    pub voicing_ratio: f32,
}

/// Compute a spectral fingerprint from raw 16-bit PCM at `sample_rate` Hz.
///
/// Uses a simple DFT over the first ≤ 512 samples to derive band energies
/// and spectral centroid.  At N=512 this is ~262 K multiplies — well under
/// 1 ms per call on any modern CPU.
pub fn compute_fingerprint(samples: &[i16], sample_rate: u32) -> VoiceFingerprint {
    if samples.is_empty() {
        return VoiceFingerprint::default();
    }

    let n = samples.len().min(512);
    let s: Vec<f32> = samples[..n].iter().map(|&x| x as f32 / 32768.0).collect();
    let sr = sample_rate as f32;
    let half = n / 2;

    // DFT magnitude spectrum.
    let mut mag = vec![0.0f32; half];
    for k in 0..half {
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        let angle = std::f32::consts::TAU * k as f32 / n as f32;
        for (i, &s_i) in s.iter().enumerate() {
            let phi = angle * i as f32;
            re += s_i * phi.cos();
            im -= s_i * phi.sin();
        }
        mag[k] = (re * re + im * im).sqrt();
    }

    let total_energy: f32 = mag.iter().sum::<f32>().max(1e-9);

    // Bin boundaries for 300 Hz and 2000 Hz.
    let bin_low = ((300.0 / sr) * n as f32) as usize;
    let bin_mid = ((2000.0 / sr) * n as f32) as usize;
    let bin_low = bin_low.clamp(1, half.saturating_sub(1));
    let bin_mid = bin_mid.clamp(bin_low + 1, half);

    let energy_low  = mag[..bin_low].iter().sum::<f32>() / total_energy;
    let energy_mid  = mag[bin_low..bin_mid].iter().sum::<f32>() / total_energy;
    let energy_high = mag[bin_mid..].iter().sum::<f32>() / total_energy;

    // Spectral centroid (normalised to [0, 1]).
    let centroid = mag.iter().enumerate()
        .map(|(k, &m)| k as f32 * m)
        .sum::<f32>()
        / (total_energy * half as f32);

    // Voiced fraction: samples with |amplitude| > 10% of full scale.
    let voiced = s.iter().filter(|&&x| x.abs() > 0.10).count();
    let voicing_ratio = voiced as f32 / s.len() as f32;

    VoiceFingerprint {
        spectral_centroid: centroid.clamp(0.0, 1.0),
        energy_low,
        energy_mid,
        energy_high,
        voicing_ratio,
    }
}

// ── Similarity ────────────────────────────────────────────────────────────────

/// Cosine similarity between two fingerprints in the 5-D feature space.
/// Returns a value in [0.0, 1.0]; higher = more similar.
pub fn similarity(a: &VoiceFingerprint, b: &VoiceFingerprint) -> f32 {
    let dot = a.spectral_centroid * b.spectral_centroid
        + a.energy_low    * b.energy_low
        + a.energy_mid    * b.energy_mid
        + a.energy_high   * b.energy_high
        + a.voicing_ratio * b.voicing_ratio;
    let norm_a = (a.spectral_centroid.powi(2)
        + a.energy_low.powi(2)
        + a.energy_mid.powi(2)
        + a.energy_high.powi(2)
        + a.voicing_ratio.powi(2))
    .sqrt();
    let norm_b = (b.spectral_centroid.powi(2)
        + b.energy_low.powi(2)
        + b.energy_mid.powi(2)
        + b.energy_high.powi(2)
        + b.voicing_ratio.powi(2))
    .sqrt();
    if norm_a < 1e-9 || norm_b < 1e-9 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
}

// ── Playback suppression gain ─────────────────────────────────────────────────

/// Returns a gain in [0.0, 1.0] that scales wake confidence when the frame's
/// fingerprint closely matches the stored TTS fingerprint.
///
/// - `sim` is cosine similarity vs. stored TTS fingerprint.
/// - Suppression is strongest in `Presentation` mode (speaker actively playing).
pub fn playback_suppression_gain(sim: f32, mode: RuntimeMode) -> f32 {
    match mode {
        RuntimeMode::Presentation => {
            if sim > 0.80 { 0.10 }
            else if sim > 0.60 { 0.50 }
            else { 1.0 }
        }
        RuntimeMode::Noisy => {
            if sim > 0.90 { 0.50 } else { 1.0 }
        }
        _ => 1.0,
    }
}

// ── Stored TTS fingerprint ────────────────────────────────────────────────────

static TTS_FINGERPRINT: Lazy<Mutex<Option<VoiceFingerprint>>> =
    Lazy::new(|| Mutex::new(None));

/// Store a fingerprint captured from TTS output for future self-hearing checks.
/// Should be called when TTS playback starts, with the first PCM chunk.
pub fn store_tts_fingerprint(fp: VoiceFingerprint) {
    *TTS_FINGERPRINT.lock() = Some(fp);
}

/// Compare an incoming frame fingerprint against the stored TTS fingerprint.
/// Returns 0.0 if no TTS fingerprint has been stored yet.
pub fn tts_similarity(incoming: &VoiceFingerprint) -> f32 {
    match TTS_FINGERPRINT.lock().as_ref() {
        Some(stored) => similarity(stored, incoming),
        None => 0.0,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_silence_is_default() {
        let samples = vec![0i16; 256];
        let fp = compute_fingerprint(&samples, 16000);
        // All-zero input: total_energy clamps to 1e-9, ratios should be zero.
        assert!(fp.energy_low >= 0.0 && fp.energy_low <= 1.0);
        assert!(fp.voicing_ratio == 0.0);
    }

    #[test]
    fn similarity_identical_is_one() {
        let samples: Vec<i16> = (0..256).map(|i| (i as i16) * 100).collect();
        let fp = compute_fingerprint(&samples, 16000);
        let s = similarity(&fp, &fp);
        assert!((s - 1.0).abs() < 1e-5, "identical fingerprints must have sim=1.0");
    }

    #[test]
    fn similarity_zero_vs_nonzero_is_zero() {
        let empty = VoiceFingerprint::default();
        let samples: Vec<i16> = (0..256).map(|i| (i as i16) * 100).collect();
        let fp = compute_fingerprint(&samples, 16000);
        let s = similarity(&empty, &fp);
        assert_eq!(s, 0.0);
    }

    #[test]
    fn playback_suppression_presentation_high_sim() {
        let gain = playback_suppression_gain(0.85, RuntimeMode::Presentation);
        assert!(gain < 0.2, "high similarity in Presentation must be strongly suppressed");
    }

    #[test]
    fn playback_suppression_normal_mode_no_effect() {
        let gain = playback_suppression_gain(0.99, RuntimeMode::Normal);
        assert_eq!(gain, 1.0);
    }
}
