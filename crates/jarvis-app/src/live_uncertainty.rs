//! Live uncertainty calibration — continuously recalibrates confidence across
//! six runtime dimensions using evidence from active runtime modules.
//! Publishes UncertaintyShift events when a dimension moves significantly.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static CALIBRATIONS_RUN:   AtomicU64 = AtomicU64::new(0);
pub static SHIFTS_DETECTED:    AtomicU64 = AtomicU64::new(0);
pub static CRITICAL_SHIFTS:    AtomicU64 = AtomicU64::new(0);

const SHIFT_THRESHOLD: f32 = 0.15;   // publish event if |new - old| ≥ this

// ── Calibration snapshot ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CalibrationSnapshot {
    pub ocr_confidence:       f32,
    pub planner_confidence:   f32,
    pub prediction_confidence: f32,
    pub env_stability:        f32,
    pub workflow_reliability: f32,
    pub causal_confidence:    f32,
    pub overall:              f32,
    pub critical_count:       usize,
    pub ts_ms:                u64,
}

impl CalibrationSnapshot {
    pub fn is_healthy(&self) -> bool {
        self.overall >= 0.4 && self.critical_count == 0
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct LiveUncState {
    prev: Option<CalibrationSnapshot>,
}

static STATE: Lazy<Mutex<LiveUncState>> = Lazy::new(|| Mutex::new(LiveUncState {
    prev: None,
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Run a full calibration pass.  Publishes events for significant shifts.
pub fn calibrate() -> CalibrationSnapshot {
    CALIBRATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    // Gather evidence from runtime modules
    let unc_snap = crate::uncertainty_engine::sample();

    let ocr_unc = unc_snap.readings.iter()
        .find(|r| r.dimension == "attention")
        .map(|r| r.combined).unwrap_or(0.5);
    let planner_unc = unc_snap.readings.iter()
        .find(|r| r.dimension == crate::uncertainty_engine::DIM_PLANNER)
        .map(|r| r.combined).unwrap_or(0.5);
    let pred_unc = unc_snap.readings.iter()
        .find(|r| r.dimension == crate::uncertainty_engine::DIM_PREDICTION)
        .map(|r| r.combined).unwrap_or(0.5);
    let causal_unc = unc_snap.readings.iter()
        .find(|r| r.dimension == crate::uncertainty_engine::DIM_CAUSAL)
        .map(|r| r.combined).unwrap_or(0.5);
    let workflow_unc = unc_snap.readings.iter()
        .find(|r| r.dimension == crate::uncertainty_engine::DIM_WORKFLOW)
        .map(|r| r.combined).unwrap_or(0.5);
    let recovery_unc = unc_snap.readings.iter()
        .find(|r| r.dimension == crate::uncertainty_engine::DIM_RECOVERY)
        .map(|r| r.combined).unwrap_or(0.5);

    // Convert uncertainty → confidence (1 - uncertainty)
    let ocr_conf       = (1.0 - ocr_unc).clamp(0.0, 1.0);
    let planner_conf   = (1.0 - planner_unc).clamp(0.0, 1.0);
    let pred_conf      = (1.0 - pred_unc).clamp(0.0, 1.0);
    let env_stability  = (1.0 - recovery_unc).clamp(0.0, 1.0);
    let workflow_rel   = (1.0 - workflow_unc).clamp(0.0, 1.0);
    let causal_conf    = (1.0 - causal_unc).clamp(0.0, 1.0);

    let overall = (ocr_conf + planner_conf + pred_conf + env_stability + workflow_rel + causal_conf) / 6.0;

    let critical_count = [ocr_conf, planner_conf, pred_conf, env_stability, workflow_rel, causal_conf]
        .iter().filter(|&&v| v < 0.3).count();

    if critical_count > 0 {
        CRITICAL_SHIFTS.fetch_add(critical_count as u64, Ordering::Relaxed);
    }

    let snap = CalibrationSnapshot {
        ocr_confidence: ocr_conf,
        planner_confidence: planner_conf,
        prediction_confidence: pred_conf,
        env_stability,
        workflow_reliability: workflow_rel,
        causal_confidence: causal_conf,
        overall,
        critical_count,
        ts_ms: now,
    };

    // Detect and publish shifts vs previous calibration
    let pairs: &[(&str, f32, f32)] = &[];  // populated below via prev
    if let Ok(mut s) = STATE.lock() {
        if let Some(prev) = &s.prev {
            let dims = [
                ("ocr",       prev.ocr_confidence,          ocr_conf),
                ("planner",   prev.planner_confidence,      planner_conf),
                ("prediction",prev.prediction_confidence,   pred_conf),
                ("env",       prev.env_stability,           env_stability),
                ("workflow",  prev.workflow_reliability,    workflow_rel),
                ("causal",    prev.causal_confidence,       causal_conf),
            ];
            for (name, old, new_v) in dims {
                let delta = (new_v - old).abs();
                if delta >= SHIFT_THRESHOLD {
                    SHIFTS_DETECTED.fetch_add(1, Ordering::Relaxed);
                    crate::meta_event_bus::publish(
                        crate::meta_event_bus::MetaEvent::UncertaintyShift {
                            dimension: name.to_string(),
                            old,
                            new: new_v,
                        }
                    );
                    crate::meta_event_bus::publish(
                        crate::meta_event_bus::MetaEvent::UncertaintyRecalib {
                            dimension: name.to_string(),
                            value: new_v,
                        }
                    );
                }
            }
        }
        s.prev = Some(snap.clone());
    }
    let _ = pairs; // suppress unused warning

    snap
}

/// Latest calibration snapshot without re-running a full calibration.
pub fn latest() -> Option<CalibrationSnapshot> {
    STATE.lock().ok().and_then(|s| s.prev.clone())
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
    fn calibration_runs_and_returns_snapshot() {
        let snap = calibrate();
        assert!(snap.overall >= 0.0 && snap.overall <= 1.0);
        assert!(CALIBRATIONS_RUN.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn second_calibration_compares_to_prev() {
        let _ = calibrate();
        let snap2 = calibrate();
        // should have prev now
        assert!(latest().is_some());
        assert!(snap2.ocr_confidence >= 0.0);
    }
}
