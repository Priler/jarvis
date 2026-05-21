//! Cognitive confidence — unified confidence model aggregating reasoning quality,
//! uncertainty, causal link reliability, and simulation pass rates.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static CONFIDENCE_SAMPLES:   AtomicU64 = AtomicU64::new(0);
pub static LOW_CONFIDENCE_OBS:   AtomicU64 = AtomicU64::new(0);
pub static CONFIDENCE_RECOVERED: AtomicU64 = AtomicU64::new(0);

const MAX_CONFIDENCE_HISTORY: usize = 80;
const LOW_CONFIDENCE_THRESHOLD: f32 = 0.45;

// ── Confidence breakdown ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfidenceBreakdown {
    pub reasoning:   f32,
    pub uncertainty: f32,   // inverted: high uncertainty → low confidence
    pub causal:      f32,
    pub simulation:  f32,
    pub stability:   f32,
    pub overall:     f32,
    pub ts_ms:       u64,
}

impl ConfidenceBreakdown {
    pub fn is_low(&self) -> bool { self.overall < LOW_CONFIDENCE_THRESHOLD }
    pub fn is_critical(&self) -> bool { self.overall < 0.25 }
    pub fn label(&self) -> &'static str {
        match (self.overall * 10.0) as u32 {
            0..=2 => "critical",
            3..=4 => "low",
            5..=6 => "moderate",
            7..=8 => "high",
            _     => "very_high",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ConfidenceState {
    history: Vec<ConfidenceBreakdown>,
}

static STATE: Lazy<Mutex<ConfidenceState>> = Lazy::new(|| Mutex::new(ConfidenceState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn measure() -> ConfidenceBreakdown {
    CONFIDENCE_SAMPLES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let reasoning = crate::reasoning_analyzer::latest()
        .map(|q| q.overall)
        .unwrap_or(0.6);

    let uncertainty_raw = crate::uncertainty_engine::overall_uncertainty();
    let uncertainty_conf = (1.0 - uncertainty_raw).clamp(0.0, 1.0);

    let causal_links = crate::causal_reasoner::reliable_links().len() as f32;
    let causal = (causal_links / 10.0).min(1.0).max(0.3);

    let simulation = crate::strategy_simulator::pass_rate();

    let stability = if crate::cognitive_drift_control::is_frozen() { 0.3 } else {
        let drift = crate::cognitive_drift_control::recent_events(5).len();
        (1.0 - drift as f32 / 5.0).clamp(0.3, 1.0)
    };

    let overall = (reasoning * 0.25 + uncertainty_conf * 0.20 + causal * 0.20
        + simulation * 0.20 + stability * 0.15).clamp(0.0, 1.0);

    let bd = ConfidenceBreakdown { reasoning, uncertainty: uncertainty_conf, causal, simulation, stability, overall, ts_ms: now };

    if bd.is_low() { LOW_CONFIDENCE_OBS.fetch_add(1, Ordering::Relaxed); }

    if let Ok(mut s) = STATE.lock() {
        let prev_low = s.history.last().map(|p| p.is_low()).unwrap_or(true);
        if prev_low && !bd.is_low() { CONFIDENCE_RECOVERED.fetch_add(1, Ordering::Relaxed); }
        if s.history.len() >= MAX_CONFIDENCE_HISTORY { s.history.remove(0); }
        s.history.push(bd.clone());
    }

    bd
}

pub fn overall() -> f32 {
    latest().map(|b| b.overall).unwrap_or(0.5)
}

pub fn latest() -> Option<ConfidenceBreakdown> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn average_overall(window: usize) -> f32 {
    STATE.lock().map(|s| {
        let slice: Vec<f32> = s.history.iter().rev().take(window).map(|b| b.overall).collect();
        if slice.is_empty() { return 0.5; }
        slice.iter().sum::<f32>() / slice.len() as f32
    }).unwrap_or(0.5)
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); }
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
    fn measure_returns_bounded_overall() {
        let b = measure();
        assert!(b.overall >= 0.0 && b.overall <= 1.0);
    }

    #[test]
    fn confidence_samples_counter_increments() {
        let before = CONFIDENCE_SAMPLES.load(Ordering::Relaxed);
        measure();
        assert!(CONFIDENCE_SAMPLES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn label_returns_valid_string() {
        let b = measure();
        let l = b.label();
        assert!(["critical", "low", "moderate", "high", "very_high"].contains(&l));
    }

    #[test]
    fn average_overall_bounded() {
        measure();
        let avg = average_overall(5);
        assert!(avg >= 0.0 && avg <= 1.0);
    }

    #[test]
    fn latest_is_some_after_measure() {
        measure();
        assert!(latest().is_some());
    }

    #[test]
    fn is_low_threshold() {
        let b = ConfidenceBreakdown {
            reasoning: 0.2, uncertainty: 0.2, causal: 0.2,
            simulation: 0.2, stability: 0.2, overall: 0.2, ts_ms: 0,
        };
        assert!(b.is_low());
    }
}
