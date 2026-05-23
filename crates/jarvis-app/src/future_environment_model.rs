//! Future environment model — predicts environment evolution, workflow
//! degradation, instability cascades, cognition pressure growth, and
//! strategic bottlenecks over a configurable horizon.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── EnvironmentForecast ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnvironmentForecast {
    pub horizon_ticks:          u32,
    pub predicted_instability:  f32,
    pub memory_pressure_trend:  f32,
    pub scheduler_stability:    f32,
    pub planner_reliability:    f32,
    pub cognition_load_trend:   f32,
    pub cascade_risk:           bool,
    pub bottleneck_components:  Vec<String>,
    pub ts_ms:                  u64,
}

impl EnvironmentForecast {
    /// True when the forecast indicates a likely deterioration.
    pub fn is_deteriorating(&self) -> bool {
        self.predicted_instability > 0.55 || self.cascade_risk
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<EnvironmentForecast>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

// ── Forecast logic ────────────────────────────────────────────────────────────

/// Predict environment evolution over `horizon_ticks` ticks.
/// Uses current signals and extrapolates linearly with decay.
pub fn forecast(horizon_ticks: u32) -> EnvironmentForecast {
    let unc      = crate::generalized_uncertainty::profile();
    let resource = crate::abstract_resource_reasoner::sample();
    let prob     = crate::probabilistic_stability::check();
    let sem      = crate::semantic_stability::check();
    let conf     = crate::confidence_reasoner::assess();

    // Growth rate: each tick multiplies instability by a small drift factor
    let drift_factor = 1.0 + (unc.overall * 0.08).min(0.15);
    let h = horizon_ticks as f32;

    // Predicted instability after h ticks (exponential growth, clamped)
    let base_instability = (unc.overall + prob.instability_score) / 2.0;
    let predicted_instability = (base_instability * drift_factor.powf(h * 0.1))
        .clamp(0.0, 1.0);

    // Memory pressure trend: resource overall extrapolated
    let memory_pressure_trend = (resource.overall + (resource.overall * 0.05 * h * 0.1))
        .clamp(0.0, 1.0);

    // Scheduler stability: degrades with load
    let avg_load = crate::adaptive_topology::avg_load();
    let scheduler_stability = (1.0 - avg_load * drift_factor.powf(h * 0.05))
        .clamp(0.0, 1.0);

    // Planner reliability: from confidence reasoner, degraded by horizon
    let planner_reliability = (conf.planner_confidence - unc.planner_uncertainty * h * 0.02)
        .clamp(0.0, 1.0);

    // Cognition load trend: monotonically increasing under instability
    let cognition_load_trend = (avg_load + unc.overall * h * 0.01)
        .clamp(0.0, 1.0);

    // Cascade risk: memory pressure high → scheduler unstable → planner degrades
    let cascade_risk = memory_pressure_trend > 0.65
        && scheduler_stability < 0.45
        && planner_reliability < 0.50;

    // Bottleneck identification
    let mut bottlenecks: Vec<String> = Vec::new();
    if memory_pressure_trend > 0.65 { bottlenecks.push("memory_pressure".into()); }
    if scheduler_stability < 0.40   { bottlenecks.push("scheduler_instability".into()); }
    if planner_reliability < 0.45   { bottlenecks.push("planner_degradation".into()); }
    if sem.instability_score > 0.55 { bottlenecks.push("semantic_instability".into()); }
    if unc.causal_uncertainty > 0.60{ bottlenecks.push("causal_uncertainty".into()); }

    let fc = EnvironmentForecast {
        horizon_ticks,
        predicted_instability,
        memory_pressure_trend,
        scheduler_stability,
        planner_reliability,
        cognition_load_trend,
        cascade_risk,
        bottleneck_components: bottlenecks,
        ts_ms: ts_now(),
    };

    let mut h_store = HISTORY.lock().unwrap();
    if h_store.len() >= MAX_HISTORY { h_store.remove(0); }
    h_store.push(fc.clone());

    crate::world_evolution_observability::record(
        crate::world_evolution_observability::WorldSimEvent::FuturePredicted {
            horizon_ticks: fc.horizon_ticks,
            instability:   fc.predicted_instability,
        }
    );

    fc
}

pub fn latest() -> Option<EnvironmentForecast> {
    HISTORY.lock().unwrap().last().cloned()
}

pub fn recent(n: usize) -> Vec<EnvironmentForecast> {
    HISTORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forecast_no_panic() {
        let fc = forecast(10);
        assert!(fc.predicted_instability >= 0.0 && fc.predicted_instability <= 1.0);
    }

    #[test]
    fn forecast_horizon_1_bounded() {
        let fc = forecast(1);
        assert!(fc.scheduler_stability >= 0.0);
        assert!(fc.planner_reliability >= 0.0);
    }

    #[test]
    fn forecast_long_horizon_bounded() {
        let fc = forecast(100);
        assert!(fc.predicted_instability <= 1.0);
        assert!(fc.cognition_load_trend <= 1.0);
    }
}
