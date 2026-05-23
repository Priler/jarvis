//! Cognitive stability — detects reasoning instability by monitoring oscillation
//! in quality scores, confidence swings, and drift event frequency.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static STABILITY_CHECKS:     AtomicU64 = AtomicU64::new(0);
pub static INSTABILITY_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static STABILITY_RESTORED:   AtomicU64 = AtomicU64::new(0);

const MAX_STABILITY_HISTORY: usize = 80;
const OSCILLATION_WINDOW:    usize = 5;
const INSTABILITY_THRESHOLD: f32   = 0.25;   // swing magnitude triggers instability

// ── Stability reading ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StabilityReading {
    pub confidence_swing:  f32,
    pub quality_swing:     f32,
    pub drift_frequency:   f32,
    pub oscillation_score: f32,
    pub is_stable:         bool,
    pub ts_ms:             u64,
}

impl StabilityReading {
    pub fn is_unstable(&self) -> bool { !self.is_stable }
    pub fn risk_level(&self) -> &'static str {
        match (self.oscillation_score * 10.0) as u32 {
            0..=2 => "low",
            3..=5 => "moderate",
            6..=7 => "high",
            _     => "critical",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct StabilityState {
    history:            Vec<StabilityReading>,
    confidence_window:  Vec<f32>,
    quality_window:     Vec<f32>,
}

static STATE: Lazy<Mutex<StabilityState>> = Lazy::new(|| Mutex::new(StabilityState {
    history:           Vec::new(),
    confidence_window: Vec::new(),
    quality_window:    Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn check() -> StabilityReading {
    STABILITY_CHECKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let current_confidence = crate::cognitive_confidence::overall();
    let current_quality    = crate::execution_quality::latest().map(|q| q.overall).unwrap_or(0.6);
    let drift_events       = crate::cognitive_drift_control::recent_events(OSCILLATION_WINDOW).len();

    let (confidence_swing, quality_swing) = STATE.lock().map(|mut s| {
        // Update windows
        if s.confidence_window.len() >= OSCILLATION_WINDOW { s.confidence_window.remove(0); }
        s.confidence_window.push(current_confidence);
        if s.quality_window.len() >= OSCILLATION_WINDOW { s.quality_window.remove(0); }
        s.quality_window.push(current_quality);

        let cswing = window_swing(&s.confidence_window);
        let qswing = window_swing(&s.quality_window);
        (cswing, qswing)
    }).unwrap_or((0.0, 0.0));

    let drift_frequency = (drift_events as f32 / OSCILLATION_WINDOW as f32).min(1.0);
    let oscillation_score = (confidence_swing * 0.40 + quality_swing * 0.35 + drift_frequency * 0.25).clamp(0.0, 1.0);
    let is_stable = oscillation_score < INSTABILITY_THRESHOLD;

    let reading = StabilityReading { confidence_swing, quality_swing, drift_frequency, oscillation_score, is_stable, ts_ms: now };

    if !is_stable { INSTABILITY_DETECTED.fetch_add(1, Ordering::Relaxed); }

    if let Ok(mut s) = STATE.lock() {
        let was_unstable = s.history.last().map(|r: &StabilityReading| r.is_unstable()).unwrap_or(false);
        if was_unstable && is_stable { STABILITY_RESTORED.fetch_add(1, Ordering::Relaxed); }
        if s.history.len() >= MAX_STABILITY_HISTORY { s.history.remove(0); }
        s.history.push(reading.clone());
    }

    reading
}

pub fn latest() -> Option<StabilityReading> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn is_stable() -> bool {
    latest().map(|r| r.is_stable).unwrap_or(true)
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() {
        s.history.clear();
        s.confidence_window.clear();
        s.quality_window.clear();
    }
}

fn window_swing(window: &[f32]) -> f32 {
    if window.len() < 2 { return 0.0; }
    let min = window.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = window.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    (max - min).clamp(0.0, 1.0)
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_returns_bounded_oscillation() {
        let r = check();
        assert!(r.oscillation_score >= 0.0 && r.oscillation_score <= 1.0);
    }

    #[test]
    fn stability_checks_counter_increments() {
        let before = STABILITY_CHECKS.load(Ordering::Relaxed);
        check();
        assert!(STABILITY_CHECKS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn risk_level_returns_valid_string() {
        let r = check();
        let l = r.risk_level();
        assert!(["low", "moderate", "high", "critical"].contains(&l));
    }

    #[test]
    fn is_stable_consistent_with_reading() {
        check();
        let r = latest().unwrap();
        assert_eq!(is_stable(), r.is_stable);
    }

    #[test]
    fn window_swing_zero_for_single_element() {
        assert_eq!(window_swing(&[0.5]), 0.0);
    }

    #[test]
    fn window_swing_correct() {
        let swing = window_swing(&[0.2, 0.8]);
        assert!((swing - 0.6).abs() < 0.001);
    }
}
