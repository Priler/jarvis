//! World model evolution — evolves semantic structures, probabilistic graphs,
//! conceptual hierarchies, and predictive simulation models over time.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── WorldModelDelta ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorldModelDelta {
    pub semantic_delta:      f32,
    pub probabilistic_delta: f32,
    pub conceptual_delta:    f32,
    pub predictive_delta:    f32,
    pub total_evolution:     f32,
    pub is_improving:        bool,
    pub ts_ms:               u64,
}

impl WorldModelDelta {
    pub fn magnitude(&self) -> f32 {
        (self.semantic_delta.powi(2)
            + self.probabilistic_delta.powi(2)
            + self.conceptual_delta.powi(2)
            + self.predictive_delta.powi(2))
        .sqrt() / 2.0
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<WorldModelDelta>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Evolution logic ───────────────────────────────────────────────────────────

/// Compute and apply one evolution step to the world model.
pub fn evolve_world_model() -> WorldModelDelta {
    let sem      = crate::semantic_stability::check();
    let prob     = crate::probabilistic_stability::check();
    let resource = crate::abstract_resource_reasoner::sample();
    let conf     = crate::confidence_reasoner::assess();
    let unc      = crate::generalized_uncertainty::profile();

    // Semantic evolution: drift toward stability — negative delta = improvement
    let semantic_delta = (sem.instability_score - conf.semantic_reliability)
        .clamp(-0.10, 0.10);

    // Probabilistic evolution: adapt to current belief state
    let probabilistic_delta = (prob.instability_score - conf.reasoning_confidence * 0.8)
        .clamp(-0.10, 0.10);

    // Conceptual evolution: conceptual load drives adaptation
    let conceptual_delta = (resource.conceptual_load - 0.40)
        .clamp(-0.10, 0.10);

    // Predictive evolution: how much our predictive models need to shift
    let predictive_delta = (unc.overall - conf.planner_confidence)
        .clamp(-0.10, 0.10);

    let total_evolution = (semantic_delta + probabilistic_delta
        + conceptual_delta + predictive_delta).abs();
    let is_improving = semantic_delta < 0.0 && probabilistic_delta < 0.0;

    let delta = WorldModelDelta {
        semantic_delta,
        probabilistic_delta,
        conceptual_delta,
        predictive_delta,
        total_evolution,
        is_improving,
        ts_ms: ts_now(),
    };

    {
        let mut h = HISTORY.lock().unwrap();
        if h.len() >= MAX_HISTORY { h.remove(0); }
        h.push(delta.clone());
    }

    crate::world_evolution_observability::record(
        crate::world_evolution_observability::WorldSimEvent::WorldModelUpdated {
            component: "world_model".into(),
            delta:     total_evolution,
        }
    );

    crate::future_memory::store(
        crate::future_memory::FutureCategory::SemanticForecast,
        format!("sem_delta={semantic_delta:.4}_prob_delta={probabilistic_delta:.4}"),
        total_evolution,
    );

    delta
}

/// Average evolution magnitude over recent N steps.
pub fn avg_evolution_magnitude(n: usize) -> f32 {
    let h = HISTORY.lock().unwrap();
    let slice: Vec<f32> = h.iter().rev().take(n).map(|d| d.magnitude()).collect();
    if slice.is_empty() { return 0.0; }
    slice.iter().sum::<f32>() / slice.len() as f32
}

pub fn recent(n: usize) -> Vec<WorldModelDelta> {
    HISTORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

pub fn latest() -> Option<WorldModelDelta> {
    HISTORY.lock().unwrap().last().cloned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evolve_world_model_no_panic() {
        let d = evolve_world_model();
        assert!(d.total_evolution >= 0.0);
    }

    #[test]
    fn deltas_bounded() {
        let d = evolve_world_model();
        assert!(d.semantic_delta >= -0.10 && d.semantic_delta <= 0.10);
        assert!(d.probabilistic_delta >= -0.10 && d.probabilistic_delta <= 0.10);
    }

    #[test]
    fn avg_evolution_magnitude_non_negative() {
        let _ = evolve_world_model();
        assert!(avg_evolution_magnitude(5) >= 0.0);
    }
}
