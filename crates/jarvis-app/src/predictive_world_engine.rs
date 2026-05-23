//! Predictive world engine — simulates long-term cognition evolution,
//! planner collapse risks, routing instability, probabilistic drift,
//! and semantic degradation over configurable horizons.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── WorldPrediction ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorldPrediction {
    pub horizon_ticks:               u32,
    pub cognition_stability_forecast: f32,
    pub planner_reliability_forecast: f32,
    pub routing_quality_forecast:     f32,
    pub semantic_drift_forecast:      f32,
    pub probabilistic_drift_forecast: f32,
    pub overall_health_forecast:      f32,
    pub critical_risks:               Vec<String>,
    pub ts_ms:                        u64,
}

impl WorldPrediction {
    pub fn has_critical_risk(&self) -> bool { !self.critical_risks.is_empty() }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<WorldPrediction>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Prediction logic ──────────────────────────────────────────────────────────

/// Simulate the world state over `horizon_ticks` and return a forecast.
pub fn predict_world(horizon_ticks: u32) -> WorldPrediction {
    let unc      = crate::generalized_uncertainty::profile();
    let sem      = crate::semantic_stability::check();
    let prob     = crate::probabilistic_stability::check();
    let resource = crate::abstract_resource_reasoner::sample();
    let conf     = crate::confidence_reasoner::assess();
    let avg_load = crate::adaptive_topology::avg_load();
    let h        = horizon_ticks as f32;

    // Decay functions: stability decays faster under high uncertainty
    let decay_rate = (0.01 + unc.overall * 0.04).min(0.10);
    let decay      = (-decay_rate * h).exp();

    // Cognition stability: decays from current oscillation pressure
    let cog = crate::cognitive_stability::check();
    let cognition_stability_forecast = (conf.reasoning_confidence * decay
        - cog.oscillation_score * h * 0.01)
        .clamp(0.0, 1.0);

    // Planner reliability
    let planner_reliability_forecast = (conf.planner_confidence * decay
        - unc.planner_uncertainty * h * 0.015)
        .clamp(0.0, 1.0);

    // Routing quality: degrades with load growth
    let routing_quality_forecast = ((1.0 - avg_load) * decay
        - unc.overall * h * 0.005)
        .clamp(0.0, 1.0);

    // Semantic drift: instability compounds over time
    let semantic_drift_forecast = (sem.instability_score
        + unc.semantic_uncertainty * h * 0.02)
        .clamp(0.0, 1.0);

    // Probabilistic drift: belief instability grows if not corrected
    let probabilistic_drift_forecast = (prob.instability_score
        + unc.overall * h * 0.015)
        .clamp(0.0, 1.0);

    // Overall health: composite
    let overall_health_forecast = (
        cognition_stability_forecast * 0.25
        + planner_reliability_forecast * 0.25
        + routing_quality_forecast * 0.20
        + (1.0 - semantic_drift_forecast) * 0.15
        + (1.0 - probabilistic_drift_forecast) * 0.15
    ).clamp(0.0, 1.0);

    // Critical risk identification
    let mut risks: Vec<String> = Vec::new();
    if planner_reliability_forecast < 0.25 {
        risks.push(format!("planner_collapse_risk: {planner_reliability_forecast:.3}"));
    }
    if routing_quality_forecast < 0.20 {
        risks.push(format!("routing_instability_risk: {routing_quality_forecast:.3}"));
    }
    if semantic_drift_forecast > 0.70 {
        risks.push(format!("semantic_degradation_risk: {semantic_drift_forecast:.3}"));
    }
    if probabilistic_drift_forecast > 0.65 {
        risks.push(format!("probabilistic_drift_risk: {probabilistic_drift_forecast:.3}"));
    }
    if resource.overall > 0.80 {
        risks.push(format!("resource_exhaustion_risk: {:.3}", resource.overall));
    }

    let pred = WorldPrediction {
        horizon_ticks,
        cognition_stability_forecast,
        planner_reliability_forecast,
        routing_quality_forecast,
        semantic_drift_forecast,
        probabilistic_drift_forecast,
        overall_health_forecast,
        critical_risks: risks,
        ts_ms: ts_now(),
    };

    // Store and journal
    {
        let mut h_store = HISTORY.lock().unwrap();
        if h_store.len() >= MAX_HISTORY { h_store.remove(0); }
        h_store.push(pred.clone());
    }

    crate::future_memory::store(
        crate::future_memory::FutureCategory::PredictedFuture,
        format!("horizon={horizon_ticks}_health={:.3}", pred.overall_health_forecast),
        1.0 - pred.overall_health_forecast,
    );

    pred
}

/// Predict cognition instability N ticks out (convenience).
pub fn predict_instability(horizon: u32) -> f32 {
    predict_world(horizon).semantic_drift_forecast
}

/// Predict planner collapse risk.
pub fn predict_planner_collapse(horizon: u32) -> f32 {
    let p = predict_world(horizon);
    1.0 - p.planner_reliability_forecast
}

pub fn recent(n: usize) -> Vec<WorldPrediction> {
    HISTORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

pub fn latest() -> Option<WorldPrediction> {
    HISTORY.lock().unwrap().last().cloned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predict_world_no_panic() {
        let p = predict_world(10);
        assert!(p.overall_health_forecast >= 0.0 && p.overall_health_forecast <= 1.0);
    }

    #[test]
    fn predict_world_short_horizon_positive() {
        let p = predict_world(1);
        assert!(p.cognition_stability_forecast >= 0.0);
        assert!(p.planner_reliability_forecast >= 0.0);
    }

    #[test]
    fn predict_world_long_horizon_bounded() {
        let p = predict_world(500);
        assert!(p.semantic_drift_forecast <= 1.0);
        assert!(p.overall_health_forecast >= 0.0);
    }

    #[test]
    fn predict_instability_bounded() {
        let v = predict_instability(20);
        assert!(v >= 0.0 && v <= 1.0);
    }
}
