//! Semantic self-model — models Jarvis's own cognition state.
//! Tracks stability, planner reliability, reasoning quality,
//! inference consistency, semantic drift, and cognitive degradation.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY:     usize = 100;
const DRIFT_THRESHOLD: f32   = 0.20;   // normalisation: SemanticStability DRIFT_THRESHOLD

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── SelfModelSnapshot ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SelfModelSnapshot {
    pub cognition_stability:   f32,    // 0–1 (1 = fully stable)
    pub planner_reliability:   f32,
    pub reasoning_quality:     f32,
    pub inference_consistency: f32,
    pub semantic_drift_risk:   f32,    // 0–1 (1 = high drift)
    pub cognitive_degradation: f32,    // composite degradation score
    pub is_healthy:            bool,
    pub ts_ms:                 u64,
}

impl SelfModelSnapshot {
    pub fn degradation_label(&self) -> &'static str {
        if self.cognitive_degradation > 0.75      { "critical" }
        else if self.cognitive_degradation > 0.50 { "degraded" }
        else if self.cognitive_degradation > 0.25 { "mild" }
        else                                       { "healthy" }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<SelfModelSnapshot>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Sampling ──────────────────────────────────────────────────────────────────

pub fn sample() -> SelfModelSnapshot {
    let stability = crate::semantic_stability::check();
    let cog       = crate::cognitive_stability::check();
    let unc       = crate::uncertainty_engine::sample();
    let chains    = crate::symbolic_inference::reliable_chains();

    let cognition_stability   = if cog.is_stable {
        (1.0 - cog.oscillation_score).clamp(0.0, 1.0)
    } else {
        0.30
    };
    let semantic_drift_risk   = (stability.semantic_drift / DRIFT_THRESHOLD).clamp(0.0, 1.0);
    let reasoning_quality     = (1.0 - stability.instability_score).clamp(0.0, 1.0);
    let planner_reliability   = (1.0 - unc.overall).clamp(0.0, 1.0);
    let inference_consistency = if chains.is_empty() {
        0.50
    } else {
        let sum: f32 = chains.iter().map(|c| c.confidence).sum();
        (sum / chains.len() as f32).clamp(0.0, 1.0)
    };

    let cognitive_degradation = (stability.instability_score   * 0.40
        + cog.oscillation_score                                 * 0.25
        + semantic_drift_risk                                   * 0.20
        + (1.0 - inference_consistency)                        * 0.15)
        .clamp(0.0, 1.0);

    let is_healthy = cognitive_degradation < 0.50 && !stability.has_collapse_risk;

    let snap = SelfModelSnapshot {
        cognition_stability,
        planner_reliability,
        reasoning_quality,
        inference_consistency,
        semantic_drift_risk,
        cognitive_degradation,
        is_healthy,
        ts_ms: ts_now(),
    };

    let mut h = HISTORY.lock().unwrap();
    if h.len() >= MAX_HISTORY { h.remove(0); }
    h.push(snap.clone());
    snap
}

pub fn latest() -> Option<SelfModelSnapshot> {
    HISTORY.lock().unwrap().last().cloned()
}

pub fn history(n: usize) -> Vec<SelfModelSnapshot> {
    let h = HISTORY.lock().unwrap();
    h.iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_valid_snapshot() {
        let snap = sample();
        assert!(snap.cognitive_degradation >= 0.0 && snap.cognitive_degradation <= 1.0);
        assert!(snap.cognition_stability   >= 0.0);
        assert!(snap.reasoning_quality     >= 0.0);
    }

    #[test]
    fn degradation_label_healthy() {
        let snap = SelfModelSnapshot {
            cognition_stability: 0.9, planner_reliability: 0.8, reasoning_quality: 0.9,
            inference_consistency: 0.85, semantic_drift_risk: 0.1, cognitive_degradation: 0.10,
            is_healthy: true, ts_ms: 0,
        };
        assert_eq!(snap.degradation_label(), "healthy");
    }
}
