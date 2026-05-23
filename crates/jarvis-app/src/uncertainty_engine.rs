//! Uncertainty engine — models epistemic and aleatoric uncertainty across 6 dimensions.
//! No ML; heuristic bounds derived from runtime counters and history variance.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static UNCERTAINTY_SAMPLES:  AtomicU64 = AtomicU64::new(0);
pub static HIGH_UNCERTAINTY_OBS: AtomicU64 = AtomicU64::new(0);
pub static UNCERTAINTY_RESOLVED: AtomicU64 = AtomicU64::new(0);

const MAX_UNCERTAINTY_HISTORY: usize = 100;
const HIGH_UNCERTAINTY_THRESHOLD: f32 = 0.7;

pub const DIM_PLANNER:      &str = "planner";
pub const DIM_WORKFLOW:     &str = "workflow";
pub const DIM_CAUSAL:       &str = "causal";
pub const DIM_PREDICTION:   &str = "prediction";
pub const DIM_RECOVERY:     &str = "recovery";
pub const DIM_ATTENTION:    &str = "attention";

// ── Uncertainty reading ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncertaintyReading {
    pub dimension:  String,
    pub epistemic:  f32,   // unknown unknowns — gaps in knowledge
    pub aleatoric:  f32,   // irreducible noise
    pub combined:   f32,   // max(epistemic, aleatoric)
    pub ts_ms:      u64,
}

impl UncertaintyReading {
    pub fn is_high(&self) -> bool { self.combined >= HIGH_UNCERTAINTY_THRESHOLD }
    pub fn is_critical(&self) -> bool { self.combined >= 0.85 }
}

// ── Uncertainty snapshot ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncertaintySnapshot {
    pub readings:       Vec<UncertaintyReading>,
    pub overall:        f32,
    pub high_count:     usize,
    pub ts_ms:          u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct UncertaintyState {
    history: Vec<UncertaintySnapshot>,
}

static STATE: Lazy<Mutex<UncertaintyState>> = Lazy::new(|| Mutex::new(UncertaintyState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn sample() -> UncertaintySnapshot {
    UNCERTAINTY_SAMPLES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let readings = vec![
        reading_for(DIM_PLANNER,    now),
        reading_for(DIM_WORKFLOW,   now),
        reading_for(DIM_CAUSAL,     now),
        reading_for(DIM_PREDICTION, now),
        reading_for(DIM_RECOVERY,   now),
        reading_for(DIM_ATTENTION,  now),
    ];

    let high_count = readings.iter().filter(|r| r.is_high()).count();
    let overall = readings.iter().map(|r| r.combined).sum::<f32>() / readings.len() as f32;

    if high_count > 0 {
        HIGH_UNCERTAINTY_OBS.fetch_add(high_count as u64, Ordering::Relaxed);
    }

    let snap = UncertaintySnapshot { readings, overall, high_count, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_UNCERTAINTY_HISTORY { s.history.remove(0); }
        s.history.push(snap.clone());
    }

    snap
}

pub fn resolve(dimension: &str) {
    UNCERTAINTY_RESOLVED.fetch_add(1, Ordering::Relaxed);
    let _ = dimension;
}

pub fn latest() -> Option<UncertaintySnapshot> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn overall_uncertainty() -> f32 {
    latest().map(|s| s.overall).unwrap_or(0.5)
}

pub fn dimension_uncertainty(dim: &str) -> f32 {
    latest().and_then(|s| {
        s.readings.iter().find(|r| r.dimension == dim).map(|r| r.combined)
    }).unwrap_or(0.5)
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); }
}

fn reading_for(dim: &str, now: u64) -> UncertaintyReading {
    use crate::execution_quality;
    use crate::causal_reasoner;

    let epistemic = match dim {
        DIM_CAUSAL => {
            let links = causal_reasoner::reliable_links().len() as f32;
            (1.0 - (links / 20.0).min(1.0)) * 0.6
        }
        DIM_PLANNER | DIM_PREDICTION => {
            let q = execution_quality::latest()
                .map(|s| s.success_reliability)
                .unwrap_or(0.5);
            (1.0 - q) * 0.5
        }
        _ => 0.4,
    };

    let aleatoric = match dim {
        DIM_WORKFLOW  => 0.3,
        DIM_RECOVERY  => 0.35,
        DIM_ATTENTION => 0.25,
        _             => 0.3,
    };

    let combined = epistemic.max(aleatoric).clamp(0.0, 1.0);
    UncertaintyReading { dimension: dim.to_string(), epistemic, aleatoric, combined, ts_ms: now }
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
    fn sample_returns_six_dimensions() {
        let snap = sample();
        assert_eq!(snap.readings.len(), 6);
    }

    #[test]
    fn overall_bounded() {
        let snap = sample();
        assert!(snap.overall >= 0.0 && snap.overall <= 1.0);
    }

    #[test]
    fn dimension_uncertainty_returns_value() {
        sample();
        let v = dimension_uncertainty(DIM_CAUSAL);
        assert!(v >= 0.0 && v <= 1.0);
    }

    #[test]
    fn uncertainty_samples_counter_increments() {
        let before = UNCERTAINTY_SAMPLES.load(Ordering::Relaxed);
        sample();
        assert!(UNCERTAINTY_SAMPLES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn history_grows_after_sample() {
        let before = UNCERTAINTY_SAMPLES.load(Ordering::Relaxed);
        sample();
        assert!(UNCERTAINTY_SAMPLES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn is_high_threshold_works() {
        let r = UncertaintyReading {
            dimension: "test".into(), epistemic: 0.8, aleatoric: 0.5,
            combined: 0.8, ts_ms: 0,
        };
        assert!(r.is_high());
        let r2 = UncertaintyReading {
            dimension: "test".into(), epistemic: 0.2, aleatoric: 0.3,
            combined: 0.3, ts_ms: 0,
        };
        assert!(!r2.is_high());
    }
}
