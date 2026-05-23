//! Abstract world generator — creates and maintains abstract operational world
//! states representing generalized simulation environments.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static WORLDS_GENERATED: AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── WorldState ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldState {
    Stable,
    Stressed,
    Unstable,
    Critical,
}

impl WorldState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Stable   => "stable",
            Self::Stressed => "stressed",
            Self::Unstable => "unstable",
            Self::Critical => "critical",
        }
    }

    pub fn from_instability(score: f32) -> Self {
        match score {
            s if s < 0.30 => Self::Stable,
            s if s < 0.55 => Self::Stressed,
            s if s < 0.75 => Self::Unstable,
            _              => Self::Critical,
        }
    }
}

// ── AbstractWorld ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AbstractWorld {
    pub id:                  u64,
    pub state:               WorldState,
    pub semantic_coherence:  f32,
    pub cognitive_load:      f32,
    pub routing_efficiency:  f32,
    pub instability_score:   f32,
    pub is_hypothetical:     bool,   // true for perturbed/what-if worlds
    pub ts_ms:               u64,
}

impl AbstractWorld {
    pub fn is_safe_for_simulation(&self) -> bool {
        self.instability_score < 0.75 && self.state != WorldState::Critical
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct WorldStore {
    worlds: Vec<AbstractWorld>,
    seq:    u64,
}

impl WorldStore {
    fn new() -> Self { WorldStore { worlds: Vec::new(), seq: 0 } }
}

static STORE: Lazy<Mutex<WorldStore>> = Lazy::new(|| Mutex::new(WorldStore::new()));

// ── Generation logic ──────────────────────────────────────────────────────────

fn build_world(perturbation: f32, hypothetical: bool, store: &mut WorldStore) -> AbstractWorld {
    let unc      = crate::generalized_uncertainty::profile();
    let sem      = crate::semantic_stability::check();
    let resource = crate::abstract_resource_reasoner::sample();
    let avg_load = crate::adaptive_topology::avg_load();

    let semantic_coherence = (1.0 - sem.instability_score - perturbation * 0.5)
        .clamp(0.0, 1.0);
    let cognitive_load = (avg_load + perturbation * 0.3).clamp(0.0, 1.0);
    let routing_efficiency = (1.0 - unc.overall - perturbation * 0.2).clamp(0.0, 1.0);
    let instability_score = ((unc.overall + sem.instability_score + resource.overall) / 3.0
        + perturbation * 0.4).clamp(0.0, 1.0);

    store.seq += 1;
    let id = store.seq;

    AbstractWorld {
        id,
        state: WorldState::from_instability(instability_score),
        semantic_coherence,
        cognitive_load,
        routing_efficiency,
        instability_score,
        is_hypothetical: hypothetical,
        ts_ms: ts_now(),
    }
}

/// Generate a world snapshot reflecting current actual system state.
pub fn generate_current_world() -> AbstractWorld {
    let mut s = STORE.lock().unwrap();
    let w = build_world(0.0, false, &mut s);
    if s.worlds.len() >= MAX_HISTORY { s.worlds.remove(0); }
    s.worlds.push(w.clone());
    WORLDS_GENERATED.fetch_add(1, Ordering::Relaxed);

    crate::world_evolution_observability::record(
        crate::world_evolution_observability::WorldSimEvent::WorldModelUpdated {
            component: "current_world".into(),
            delta:     w.instability_score,
        }
    );
    w
}

/// Generate a hypothetical world by applying a perturbation (0–1) to current state.
pub fn generate_hypothetical(perturbation: f32) -> AbstractWorld {
    let p = perturbation.clamp(0.0, 1.0);
    let mut s = STORE.lock().unwrap();
    let w = build_world(p, true, &mut s);
    if s.worlds.len() >= MAX_HISTORY { s.worlds.remove(0); }
    s.worlds.push(w.clone());
    WORLDS_GENERATED.fetch_add(1, Ordering::Relaxed);
    w
}

pub fn recent(n: usize) -> Vec<AbstractWorld> {
    STORE.lock().unwrap().worlds.iter().rev().take(n).cloned().collect()
}

pub fn count() -> usize { STORE.lock().unwrap().worlds.len() }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_current_world_valid() {
        let w = generate_current_world();
        assert!(w.instability_score >= 0.0 && w.instability_score <= 1.0);
        assert!(!w.is_hypothetical);
    }

    #[test]
    fn generate_hypothetical_bounded() {
        let w = generate_hypothetical(0.5);
        assert!(w.is_hypothetical);
        assert!(w.instability_score <= 1.0);
    }

    #[test]
    fn world_state_from_instability() {
        assert_eq!(WorldState::from_instability(0.1), WorldState::Stable);
        assert_eq!(WorldState::from_instability(0.9), WorldState::Critical);
    }
}
