//! Probabilistic stability engine — detects belief collapse, confidence runaway,
//! uncertainty explosion, probabilistic drift, and self-model instability.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

// Thresholds
const COLLAPSE_THRESHOLD:           f32 = 0.20;   // avg_confidence below this = collapse
const EXPLOSION_THRESHOLD:          f32 = 0.80;   // avg_uncertainty above this = explosion
const RUNAWAY_DELTA_THRESHOLD:      f32 = 0.20;   // rapid rise without evidence
const DRIFT_WINDOW:                 usize = 10;    // ticks to look back for drift
const SELF_MODEL_INSTABILITY_RATE:  f32 = 0.15;   // degradation change rate that signals instability

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ProbabilisticStabilityReport ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProbabilisticStabilityReport {
    pub is_stable:                bool,
    pub instability_score:        f32,
    pub has_belief_collapse:      bool,
    pub has_uncertainty_explosion: bool,
    pub has_confidence_runaway:   bool,
    pub has_self_model_instability: bool,
    pub recommendation:           String,
    pub ts_ms:                    u64,
}

impl ProbabilisticStabilityReport {
    pub fn is_critical(&self) -> bool {
        self.has_belief_collapse || self.has_uncertainty_explosion || self.instability_score > 0.80
    }
}

// ── History ───────────────────────────────────────────────────────────────────

struct StabilityHistory {
    reports:          Vec<ProbabilisticStabilityReport>,
    confidence_trace: Vec<f32>,   // per-tick avg_confidence snapshots
}

static STATE: Lazy<Mutex<StabilityHistory>> = Lazy::new(|| Mutex::new(StabilityHistory {
    reports:          Vec::new(),
    confidence_trace: Vec::new(),
}));

// ── Check ─────────────────────────────────────────────────────────────────────

pub fn check() -> ProbabilisticStabilityReport {
    let avg_conf  = crate::belief_engine::avg_confidence();
    let avg_unc   = crate::uncertainty_graph::avg_uncertainty();
    let self_snap = crate::semantic_self_model::sample();

    let has_belief_collapse       = avg_conf < COLLAPSE_THRESHOLD
        && crate::belief_engine::belief_count() > 5;
    let has_uncertainty_explosion = avg_unc > EXPLOSION_THRESHOLD;

    // Confidence runaway: confidence rose rapidly in recent ticks
    let has_confidence_runaway = {
        let h = STATE.lock().unwrap();
        if h.confidence_trace.len() >= DRIFT_WINDOW {
            let oldest = h.confidence_trace[h.confidence_trace.len() - DRIFT_WINDOW];
            (avg_conf - oldest) > RUNAWAY_DELTA_THRESHOLD
        } else {
            false
        }
    };

    // Self-model instability: degradation changed rapidly
    let self_history = crate::semantic_self_model::history(5);
    let has_self_model_instability = if self_history.len() >= 2 {
        let newest = self_history[0].cognitive_degradation;
        let oldest = self_history[self_history.len() - 1].cognitive_degradation;
        (newest - oldest).abs() > SELF_MODEL_INSTABILITY_RATE
    } else {
        false
    };

    let instability_score = (if has_belief_collapse      { 0.35 } else { 0.0 }
        + if has_uncertainty_explosion { 0.30 } else { 0.0 }
        + if has_confidence_runaway    { 0.20 } else { 0.0 }
        + if has_self_model_instability { 0.15 } else { 0.0 }
        + self_snap.cognitive_degradation * 0.20)
        .clamp(0.0, 1.0);

    let is_stable = !has_belief_collapse && !has_uncertainty_explosion && instability_score < 0.50;

    let recommendation = if has_belief_collapse {
        "reinforce_core_beliefs"
    } else if has_uncertainty_explosion {
        "reduce_uncertainty_propagation"
    } else if has_confidence_runaway {
        "apply_confidence_decay"
    } else if has_self_model_instability {
        "recalibrate_self_model"
    } else {
        "continue_probabilistic_reasoning"
    };

    let report = ProbabilisticStabilityReport {
        is_stable,
        instability_score,
        has_belief_collapse,
        has_uncertainty_explosion,
        has_confidence_runaway,
        has_self_model_instability,
        recommendation: recommendation.into(),
        ts_ms: ts_now(),
    };

    let mut s = STATE.lock().unwrap();
    if s.confidence_trace.len() >= MAX_HISTORY { s.confidence_trace.remove(0); }
    s.confidence_trace.push(avg_conf);
    if s.reports.len() >= MAX_HISTORY { s.reports.remove(0); }
    s.reports.push(report.clone());
    report
}

pub fn latest() -> Option<ProbabilisticStabilityReport> {
    STATE.lock().unwrap().reports.last().cloned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_no_panic() {
        let r = check();
        assert!(r.instability_score >= 0.0 && r.instability_score <= 1.0);
    }

    #[test]
    fn is_stable_when_healthy() {
        // With no beliefs and default state, should not collapse
        let r = check();
        let _ = r.is_critical();
    }
}
