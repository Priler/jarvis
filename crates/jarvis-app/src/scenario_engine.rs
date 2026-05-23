//! Scenario engine — generates operational scenarios, compares future
//! trajectories, evaluates strategic futures, and ranks simulated outcomes.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static SCENARIOS_GENERATED: AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 200;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ScenarioKind ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioKind {
    NominalOperation,
    ResourcePressure,
    CognitiveDegradation,
    SemanticCollapse,
    RoutingInstability,
}

impl ScenarioKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NominalOperation    => "nominal_operation",
            Self::ResourcePressure    => "resource_pressure",
            Self::CognitiveDegradation=> "cognitive_degradation",
            Self::SemanticCollapse    => "semantic_collapse",
            Self::RoutingInstability  => "routing_instability",
        }
    }
}

// ── Scenario ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Scenario {
    pub id:            u64,
    pub kind:          ScenarioKind,
    pub probability:   f32,
    pub risk_score:    f32,
    pub outcome_label: String,
    pub ts_ms:         u64,
}

impl Scenario {
    pub fn is_high_risk(&self) -> bool { self.risk_score > 0.65 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ScenarioStore {
    scenarios: Vec<Scenario>,
    seq:       u64,
}

impl ScenarioStore {
    fn new() -> Self { ScenarioStore { scenarios: Vec::new(), seq: 0 } }
}

static STORE: Lazy<Mutex<ScenarioStore>> = Lazy::new(|| Mutex::new(ScenarioStore::new()));

// ── Generation logic ──────────────────────────────────────────────────────────

/// Generate current operational scenarios based on live system signals.
pub fn generate_scenarios() -> Vec<Scenario> {
    let unc      = crate::generalized_uncertainty::profile();
    let prob     = crate::probabilistic_stability::check();
    let sem      = crate::semantic_stability::check();
    let resource = crate::abstract_resource_reasoner::sample();
    let avg_load = crate::adaptive_topology::avg_load();
    let ts       = ts_now();

    let mut store = STORE.lock().unwrap();
    let mut generated = Vec::new();

    let make = |seq: &mut u64, kind: ScenarioKind, probability: f32, risk: f32, label: &str| -> Scenario {
        *seq += 1;
        Scenario {
            id: *seq,
            kind,
            probability: probability.clamp(0.0, 1.0),
            risk_score:  risk.clamp(0.0, 1.0),
            outcome_label: label.to_string(),
            ts_ms: ts,
        }
    };

    // Scenario 1: Nominal operation
    {
        let prob_nominal = (1.0 - unc.overall).max(0.0);
        let risk = unc.overall * 0.30;
        let s = make(&mut store.seq, ScenarioKind::NominalOperation, prob_nominal, risk, "stable_operation");
        generated.push(s);
    }

    // Scenario 2: Resource pressure
    {
        let prob_rp = resource.overall * 0.8;
        let risk = resource.overall * 0.70 + unc.resource_uncertainty * 0.30;
        let s = make(&mut store.seq, ScenarioKind::ResourcePressure, prob_rp, risk, "resource_constrained_operation");
        generated.push(s);
    }

    // Scenario 3: Cognitive degradation
    {
        let prob_cd = prob.instability_score * 0.70;
        let risk = prob.instability_score * 0.60 + avg_load * 0.40;
        let s = make(&mut store.seq, ScenarioKind::CognitiveDegradation, prob_cd, risk, "degraded_cognition_mode");
        generated.push(s);
    }

    // Scenario 4: Semantic collapse (only if risk is non-trivial)
    if sem.instability_score > 0.15 || sem.has_collapse_risk {
        let prob_sc = sem.instability_score * 0.50 + if sem.has_collapse_risk { 0.30 } else { 0.0 };
        let risk = sem.instability_score * 0.80 + if sem.has_collapse_risk { 0.20 } else { 0.0 };
        let s = make(&mut store.seq, ScenarioKind::SemanticCollapse, prob_sc, risk, "semantic_hierarchy_collapse");
        generated.push(s);
    }

    // Scenario 5: Routing instability
    if avg_load > 0.30 || unc.overall > 0.20 {
        let prob_ri = avg_load * 0.50 + unc.overall * 0.30;
        let risk = avg_load * 0.60 + unc.causal_uncertainty * 0.40;
        let s = make(&mut store.seq, ScenarioKind::RoutingInstability, prob_ri, risk, "routing_degradation");
        generated.push(s);
    }

    // Store them
    for s in &generated {
        if store.scenarios.len() >= MAX_HISTORY { store.scenarios.remove(0); }
        store.scenarios.push(s.clone());
    }

    SCENARIOS_GENERATED.fetch_add(generated.len() as u64, Ordering::Relaxed);

    crate::world_evolution_observability::record(
        crate::world_evolution_observability::WorldSimEvent::SimulationRun {
            scenario_id: generated.first().map(|s| s.id).unwrap_or(0),
            outcome:     "scenarios_generated".into(),
            instability: unc.overall,
        }
    );

    generated
}

/// Sort scenarios by descending risk score.
pub fn rank_by_risk(scenarios: &[Scenario]) -> Vec<Scenario> {
    let mut ranked = scenarios.to_vec();
    ranked.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
    ranked
}

/// Return only high-risk scenarios from history.
pub fn high_risk_scenarios() -> Vec<Scenario> {
    STORE.lock().unwrap().scenarios.iter()
        .filter(|s| s.is_high_risk())
        .cloned()
        .collect()
}

pub fn recent(n: usize) -> Vec<Scenario> {
    STORE.lock().unwrap().scenarios.iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_scenarios_non_empty() {
        let s = generate_scenarios();
        assert!(!s.is_empty());
    }

    #[test]
    fn scenarios_probability_bounded() {
        let s = generate_scenarios();
        for sc in &s {
            assert!(sc.probability >= 0.0 && sc.probability <= 1.0);
            assert!(sc.risk_score  >= 0.0 && sc.risk_score  <= 1.0);
        }
    }

    #[test]
    fn rank_by_risk_descending() {
        let s = generate_scenarios();
        let ranked = rank_by_risk(&s);
        for pair in ranked.windows(2) {
            assert!(pair[0].risk_score >= pair[1].risk_score);
        }
    }
}
