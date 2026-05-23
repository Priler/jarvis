//! Conceptual world model — maintains an abstract view of operational state.
//! Tracks abstract dependencies, strategic fragility, and environment archetypes.
//! Derived entirely from local runtime signals; no external data.

use std::sync::Mutex;
use once_cell::sync::Lazy;

// ── AbstractState ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AbstractState {
    Stable,
    Fragile,
    Degrading { rate: f32 },
    Overloaded { dimension: String },
    Recovering,
    Unknown,
}

// ── EnvironmentArchetype ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EnvironmentArchetype {
    HighThroughput,   // many events, stable
    HighRisk,         // high uncertainty, fragile
    ResourceConstrained, // low budget, throttled
    Quiescent,        // low activity
    Transitioning,    // state in flux
}

// ── WorldModelSnapshot ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorldModelSnapshot {
    pub abstract_state:     AbstractState,
    pub archetype:          EnvironmentArchetype,
    pub strategic_fragility: f32,       // 0–1
    pub conceptual_load:    f32,        // 0–1
    pub dependency_risk:    f32,        // 0–1
    pub optimization_pressure: f32,    // 0–1
    pub ts_ms:              u64,
}

impl WorldModelSnapshot {
    pub fn is_stable(&self) -> bool {
        matches!(self.abstract_state, AbstractState::Stable)
    }
    pub fn needs_intervention(&self) -> bool {
        self.strategic_fragility > 0.7 || self.dependency_risk > 0.75
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ModelState {
    history: Vec<WorldModelSnapshot>,
}

static STATE: Lazy<Mutex<ModelState>> = Lazy::new(|| Mutex::new(ModelState {
    history: Vec::new(),
}));

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Update logic ──────────────────────────────────────────────────────────────

pub fn update() -> WorldModelSnapshot {
    let unc = crate::uncertainty_engine::sample();
    let stability = crate::cognitive_stability::check();
    let emergency = crate::resource_scheduler::is_emergency();
    let bg_suspended = crate::resource_scheduler::is_bg_suspended();
    let concepts = crate::concept_engine::reliable_concepts();
    let failure_concepts = concepts.iter()
        .filter(|c| c.kind == crate::concept_engine::ConceptKind::Failure)
        .count();

    // Use long-horizon reasoning trajectory if available
    let lh_latest = crate::long_horizon_reasoning::history(1).into_iter().next();
    let lh_degrading = lh_latest.as_ref()
        .map(|a| matches!(a.env_trajectory, crate::long_horizon_reasoning::EnvTrajectory::Degrading { rate } if rate >= 0.1))
        .unwrap_or(false);
    let lh_recovering = lh_latest.as_ref()
        .map(|a| matches!(a.env_trajectory, crate::long_horizon_reasoning::EnvTrajectory::Improving))
        .unwrap_or(false);
    let drift_rate = stability.drift_frequency; // proxy for rate of change

    // Abstract state derivation
    let abstract_state = if emergency {
        AbstractState::Overloaded { dimension: "resource_budget".to_string() }
    } else if unc.overall > 0.8 {
        AbstractState::Fragile
    } else if lh_degrading {
        AbstractState::Degrading { rate: drift_rate }
    } else if lh_recovering {
        AbstractState::Recovering
    } else if unc.overall < 0.4 && stability.is_stable {
        AbstractState::Stable
    } else {
        AbstractState::Unknown
    };

    // Environment archetype
    let archetype = if emergency || bg_suspended {
        EnvironmentArchetype::ResourceConstrained
    } else if unc.overall > 0.7 || failure_concepts >= 3 {
        EnvironmentArchetype::HighRisk
    } else if lh_degrading || lh_recovering {
        EnvironmentArchetype::Transitioning
    } else if unc.overall < 0.3 && stability.is_stable {
        EnvironmentArchetype::Quiescent
    } else {
        EnvironmentArchetype::HighThroughput
    };

    // Fragility: driven by uncertainty + failure concept count + oscillation
    let strategic_fragility = (unc.overall * 0.4
        + (failure_concepts as f32 / 5.0).min(1.0) * 0.3
        + stability.oscillation_score * 0.3)
        .clamp(0.0, 1.0);

    // Conceptual load: how many concepts are being tracked
    let conceptual_load = (concepts.len() as f32 / 50.0).min(1.0);

    // Dependency risk: derived from causal link density
    let causal_links = crate::causal_reasoner::reliable_links();
    let dependency_risk = (causal_links.len() as f32 / 20.0).min(1.0) * unc.overall;

    // Optimization pressure: high when stable and many patterns known
    let structures = crate::semantic_structures::strong_structures();
    let optimization_pressure = if stability.is_stable {
        (structures.len() as f32 / 10.0).min(1.0)
    } else { 0.0 };

    let snap = WorldModelSnapshot {
        abstract_state,
        archetype,
        strategic_fragility,
        conceptual_load,
        dependency_risk,
        optimization_pressure,
        ts_ms: ts_now(),
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(snap.clone());
    }
    snap
}

pub fn latest() -> Option<WorldModelSnapshot> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn history() -> Vec<WorldModelSnapshot> {
    STATE.lock().map(|s| s.history.clone()).unwrap_or_default()
}
