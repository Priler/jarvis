//! Cognitive drift control — detects when runtime reasoning drifts toward
//! unstable strategies, accumulates false beliefs, or reinforces bad heuristics.
//! Acts as a safety floor: if drift exceeds threshold, adaptation is frozen.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static DRIFT_CHECKS:    AtomicU64 = AtomicU64::new(0);
pub static DRIFT_DETECTED:  AtomicU64 = AtomicU64::new(0);
pub static DRIFT_FROZEN:    AtomicU64 = AtomicU64::new(0);

const DRIFT_WINDOW:       usize = 10;
const DRIFT_THRESHOLD:    f32   = 0.35;   // below this success rate → drift
const MAX_DRIFT_EVENTS:   usize = 50;

// ── Drift event ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DriftKind {
    SuccessRateDrop   { rate: f32, window: usize },
    AnomalySurge      { count: u32 },
    PredictionCollapse{ accuracy: f32 },
    AttentionChurn    { shift_rate: f32 },
    StrategyRegression{ dimension: String, score: f32 },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DriftEvent {
    pub kind:     DriftKind,
    pub severity: f32,    // 0.0–1.0
    pub ts_ms:    u64,
}

impl DriftEvent {
    pub fn is_severe(&self) -> bool { self.severity >= 0.7 }
}

// ── Drift state ───────────────────────────────────────────────────────────────

struct DriftState {
    events:        Vec<DriftEvent>,
    frozen:        bool,
    freeze_ts_ms:  u64,
}

const FREEZE_DURATION_MS: u64 = 30_000;

static STATE: Lazy<Mutex<DriftState>> = Lazy::new(|| Mutex::new(DriftState {
    events:       Vec::new(),
    frozen:       false,
    freeze_ts_ms: 0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn check() -> Vec<DriftEvent> {
    DRIFT_CHECKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    // Auto-thaw after freeze duration
    if let Ok(mut s) = STATE.lock() {
        if s.frozen && now.saturating_sub(s.freeze_ts_ms) >= FREEZE_DURATION_MS {
            s.frozen = false;
        }
    }

    let mut events = Vec::new();

    // 1. Success rate drop
    let rate = crate::cognitive_memory::recent_success_rate(DRIFT_WINDOW);
    if rate < DRIFT_THRESHOLD {
        events.push(DriftEvent {
            kind: DriftKind::SuccessRateDrop { rate, window: DRIFT_WINDOW },
            severity: 1.0 - rate,
            ts_ms: now,
        });
    }

    // 2. Anomaly surge
    let anomalies = crate::anomaly_detector::ANOMALIES_FOUND.load(Ordering::Relaxed);
    let checks    = crate::anomaly_detector::ANOMALY_CHECKS.load(Ordering::Relaxed).max(1);
    let anomaly_rate = anomalies as f32 / checks as f32;
    if anomaly_rate > 0.5 {
        events.push(DriftEvent {
            kind: DriftKind::AnomalySurge { count: anomalies as u32 },
            severity: anomaly_rate.min(1.0),
            ts_ms: now,
        });
    }

    // 3. Prediction collapse
    let verified = crate::predictive_reasoner::PREDICTIONS_VERIFIED.load(Ordering::Relaxed);
    let correct  = crate::predictive_reasoner::PREDICTIONS_CORRECT.load(Ordering::Relaxed);
    if verified >= 5 {
        let accuracy = correct as f32 / verified as f32;
        if accuracy < 0.3 {
            events.push(DriftEvent {
                kind: DriftKind::PredictionCollapse { accuracy },
                severity: 1.0 - accuracy,
                ts_ms: now,
            });
        }
    }

    // 4. Attention churn
    let evals  = crate::attention_runtime::ATTENTION_EVALUATIONS.load(Ordering::Relaxed).max(1);
    let shifts = crate::attention_runtime::ATTENTION_SHIFTS.load(Ordering::Relaxed);
    let shift_rate = shifts as f32 / evals as f32;
    if shift_rate > 0.7 {
        events.push(DriftEvent {
            kind: DriftKind::AttentionChurn { shift_rate },
            severity: shift_rate.min(1.0),
            ts_ms: now,
        });
    }

    if !events.is_empty() {
        DRIFT_DETECTED.fetch_add(events.len() as u64, Ordering::Relaxed);

        let has_severe = events.iter().any(|e| e.is_severe());
        if has_severe {
            if let Ok(mut s) = STATE.lock() {
                if !s.frozen {
                    s.frozen = true;
                    s.freeze_ts_ms = now;
                    DRIFT_FROZEN.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if let Ok(mut s) = STATE.lock() {
            for e in &events {
                if s.events.len() >= MAX_DRIFT_EVENTS { s.events.remove(0); }
                s.events.push(e.clone());
            }
        }
    }

    events
}

pub fn is_frozen() -> bool {
    STATE.lock().map(|s| s.frozen).unwrap_or(false)
}

pub fn recent_events(n: usize) -> Vec<DriftEvent> {
    STATE.lock().map(|s| {
        let len = s.events.len();
        s.events[len.saturating_sub(n)..].to_vec()
    }).unwrap_or_default()
}

pub fn event_count() -> usize {
    STATE.lock().map(|s| s.events.len()).unwrap_or(0)
}

pub fn unfreeze_for_test() {
    if let Ok(mut s) = STATE.lock() { s.frozen = false; }
}

pub fn freeze_for_test() {
    if let Ok(mut s) = STATE.lock() { s.frozen = true; }
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
    fn check_returns_vec() {
        unfreeze_for_test();
        let events = check();
        let _ = events.len();
    }

    #[test]
    fn drift_checks_increments() {
        let before = DRIFT_CHECKS.load(Ordering::Relaxed);
        check();
        assert!(DRIFT_CHECKS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn drift_event_severity_bounded() {
        check();
        for e in recent_events(20) {
            assert!(e.severity >= 0.0 && e.severity <= 1.0);
        }
    }

    #[test]
    fn is_severe_threshold() {
        let e = DriftEvent { kind: DriftKind::AnomalySurge { count: 5 }, severity: 0.8, ts_ms: 0 };
        assert!(e.is_severe());
        let e2 = DriftEvent { severity: 0.5, ..e };
        assert!(!e2.is_severe());
    }

    #[test]
    fn unfreeze_clears_flag() {
        if let Ok(mut s) = STATE.lock() { s.frozen = true; }
        unfreeze_for_test();
        assert!(!is_frozen());
    }

    #[test]
    fn event_count_non_negative() {
        let _ = event_count();
    }
}
