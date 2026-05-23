//! Topology generation — invents new cognition routing topologies, generates
//! adaptive routing structures, and synthesizes cognition architectures.
//! All generated topologies are validated before use.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

use crate::adaptive_topology::CognitionPath;

pub static TOPOLOGIES_GENERATED: AtomicU64 = AtomicU64::new(0);
pub static TOPOLOGIES_REJECTED:  AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── GeneratedTopology ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GeneratedTopology {
    pub id:                   u64,
    pub label:                String,
    pub primary_path:         CognitionPath,
    pub fallback_path:        CognitionPath,
    pub load_balance_factor:  f32,
    pub estimated_stability:  f32,
    pub is_valid:             bool,
    pub ts_ms:                u64,
}

impl GeneratedTopology {
    pub fn efficiency_score(&self) -> f32 {
        if !self.is_valid { return 0.0; }
        self.estimated_stability * self.load_balance_factor
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct TopoStore {
    topologies: Vec<GeneratedTopology>,
    seq:        u64,
}

impl TopoStore {
    fn new() -> Self { TopoStore { topologies: Vec::new(), seq: 0 } }
}

static STORE: Lazy<Mutex<TopoStore>> = Lazy::new(|| Mutex::new(TopoStore::new()));

// ── Generation logic ──────────────────────────────────────────────────────────

fn build_topology(
    store:    &mut TopoStore,
    primary:  CognitionPath,
    fallback: CognitionPath,
    label:    &str,
) -> GeneratedTopology {
    let primary_load  = crate::adaptive_topology::get_load(primary);
    let fallback_load = crate::adaptive_topology::get_load(fallback);

    let verdict = crate::synthesis_validator::validate_topology(primary_load, fallback_load);
    let estimated_stability = verdict.confidence;
    let is_valid = verdict.is_valid;

    // load_balance_factor: 1.0 if loads are equal, lower if imbalanced
    let imbalance = (primary_load - fallback_load).abs();
    let load_balance_factor = (1.0 - imbalance).clamp(0.0, 1.0);

    store.seq += 1;
    let id = store.seq;

    if is_valid {
        TOPOLOGIES_GENERATED.fetch_add(1, Ordering::Relaxed);
    } else {
        TOPOLOGIES_REJECTED.fetch_add(1, Ordering::Relaxed);
    }

    GeneratedTopology {
        id,
        label: label.to_string(),
        primary_path: primary,
        fallback_path: fallback,
        load_balance_factor,
        estimated_stability,
        is_valid,
        ts_ms: ts_now(),
    }
}

/// Generate the single best topology based on current load state.
pub fn generate_topology() -> GeneratedTopology {
    let recommended = crate::adaptive_topology::recommended_path();
    let unc = crate::generalized_uncertainty::profile();

    // Choose fallback: if high uncertainty use MetaCognition, else Lightweight
    let fallback = if unc.overall > 0.55 {
        CognitionPath::MetaCognition
    } else {
        CognitionPath::Lightweight
    };

    let mut store = STORE.lock().unwrap();
    let topo = build_topology(&mut store, recommended, fallback, "adaptive_recommended");

    if store.topologies.len() >= MAX_HISTORY { store.topologies.remove(0); }
    store.topologies.push(topo.clone());

    crate::world_evolution_observability::record(
        crate::world_evolution_observability::WorldSimEvent::TopologyGenerated {
            label:     topo.label.clone(),
            stability: topo.estimated_stability,
        }
    );

    topo
}

/// Generate N candidate topologies covering different path combinations.
pub fn generate_topology_candidates(n: usize) -> Vec<GeneratedTopology> {
    let n = n.min(6);
    let pairs: &[(CognitionPath, CognitionPath, &str)] = &[
        (CognitionPath::Symbolic,      CognitionPath::Lightweight,    "symbolic_lightweight"),
        (CognitionPath::Probabilistic, CognitionPath::MetaCognition,  "probabilistic_meta"),
        (CognitionPath::Conceptual,    CognitionPath::Hierarchical,   "conceptual_hierarchical"),
        (CognitionPath::Hierarchical,  CognitionPath::Symbolic,       "hierarchical_symbolic"),
        (CognitionPath::MetaCognition, CognitionPath::Probabilistic,  "meta_probabilistic"),
        (CognitionPath::Lightweight,   CognitionPath::Conceptual,     "lightweight_conceptual"),
    ];

    let mut store = STORE.lock().unwrap();
    let mut candidates = Vec::new();

    for (primary, fallback, label) in pairs.iter().take(n) {
        let topo = build_topology(&mut store, *primary, *fallback, label);
        if store.topologies.len() >= MAX_HISTORY { store.topologies.remove(0); }
        store.topologies.push(topo.clone());
        candidates.push(topo);
    }

    candidates
}

/// Best validated topology from history.
pub fn best_topology() -> Option<GeneratedTopology> {
    STORE.lock().unwrap().topologies.iter()
        .filter(|t| t.is_valid)
        .max_by(|a, b| a.efficiency_score().partial_cmp(&b.efficiency_score()).unwrap())
        .cloned()
}

pub fn recent(n: usize) -> Vec<GeneratedTopology> {
    STORE.lock().unwrap().topologies.iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_topology_no_panic() {
        let t = generate_topology();
        assert!(t.estimated_stability >= 0.0 && t.estimated_stability <= 1.0);
    }

    #[test]
    fn generate_topology_candidates_bounded() {
        let c = generate_topology_candidates(3);
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn load_balance_factor_bounded() {
        let t = generate_topology();
        assert!(t.load_balance_factor >= 0.0 && t.load_balance_factor <= 1.0);
    }
}
