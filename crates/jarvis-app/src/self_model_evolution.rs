//! Self-model evolution engine — tracks changes in the semantic self-model,
//! detects self-model drift, and recalibrates confidence estimates.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY:      usize = 100;
const DRIFT_THRESHOLD:  f32   = 0.15;  // significant change in cognitive_degradation

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── SelfModelDelta ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SelfModelDelta {
    pub cognition_stability_delta:   f32,
    pub reasoning_quality_delta:     f32,
    pub cognitive_degradation_delta: f32,
    pub drift_detected:              bool,
    pub ts_ms:                       u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct EvolutionState {
    deltas:    Vec<SelfModelDelta>,
    last_snap: Option<crate::semantic_self_model::SelfModelSnapshot>,
}

static STATE: Lazy<Mutex<EvolutionState>> = Lazy::new(|| Mutex::new(EvolutionState {
    deltas:    Vec::new(),
    last_snap: None,
}));

// ── Evolution ─────────────────────────────────────────────────────────────────

pub fn evolve() -> SelfModelDelta {
    let current = crate::semantic_self_model::sample();
    let mut s   = STATE.lock().unwrap();

    let delta = match &s.last_snap {
        Some(prev) => {
            let cs_delta  = current.cognition_stability   - prev.cognition_stability;
            let rq_delta  = current.reasoning_quality     - prev.reasoning_quality;
            let deg_delta = current.cognitive_degradation - prev.cognitive_degradation;
            SelfModelDelta {
                cognition_stability_delta:   cs_delta,
                reasoning_quality_delta:     rq_delta,
                cognitive_degradation_delta: deg_delta,
                drift_detected:              deg_delta.abs() > DRIFT_THRESHOLD,
                ts_ms:                       ts_now(),
            }
        }
        None => SelfModelDelta {
            cognition_stability_delta:   0.0,
            reasoning_quality_delta:     0.0,
            cognitive_degradation_delta: 0.0,
            drift_detected:              false,
            ts_ms:                       ts_now(),
        },
    };

    if s.deltas.len() >= MAX_HISTORY { s.deltas.remove(0); }
    s.deltas.push(delta.clone());
    s.last_snap = Some(current);
    delta
}

pub fn has_drift() -> bool {
    STATE.lock().unwrap().deltas.iter().rev().take(5).any(|d| d.drift_detected)
}

pub fn recent_deltas(n: usize) -> Vec<SelfModelDelta> {
    STATE.lock().unwrap().deltas.iter().rev().take(n).cloned().collect()
}

/// If drift is detected, reinforce beliefs from stable inference chains.
pub fn recalibrate_confidence() {
    if has_drift() {
        let stable_chains = crate::symbolic_inference::reliable_chains();
        for chain in stable_chains.iter().take(5) {
            crate::belief_engine::reinforce(&chain.conclusion, 0.02);
        }
        crate::probabilistic_observability::log(
            crate::probabilistic_observability::ProbabilisticEvent::SelfModelUpdated {
                component: "confidence_calibration".into(),
                old_val:   0.0,
                new_val:   0.02,
            }
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evolve_no_panic() {
        let delta = evolve();
        let _ = delta.drift_detected;
    }

    #[test]
    fn recalibrate_no_panic() {
        recalibrate_confidence();
    }
}
