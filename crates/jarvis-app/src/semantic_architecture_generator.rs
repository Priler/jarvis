//! Semantic architecture generator — generates semantic execution structures,
//! abstraction hierarchies, adaptive planning topologies, and generalized
//! cognition schemas.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static ARCHITECTURES_GENERATED: AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ArchitectureKind ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchitectureKind {
    FlatSymbolic,
    HierarchicalSemantic,
    ProbabilisticMesh,
    AdaptiveHybrid,
}

impl ArchitectureKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::FlatSymbolic          => "flat_symbolic",
            Self::HierarchicalSemantic  => "hierarchical_semantic",
            Self::ProbabilisticMesh     => "probabilistic_mesh",
            Self::AdaptiveHybrid        => "adaptive_hybrid",
        }
    }

    pub fn base_depth(&self) -> usize {
        match self {
            Self::FlatSymbolic          => 1,
            Self::HierarchicalSemantic  => 4,
            Self::ProbabilisticMesh     => 3,
            Self::AdaptiveHybrid        => 5,
        }
    }
}

// ── SemanticArchitecture ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SemanticArchitecture {
    pub id:                 u64,
    pub kind:               ArchitectureKind,
    pub depth:              usize,
    pub semantic_coherence: f32,
    pub estimated_load:     f32,
    pub is_stable:          bool,
    pub ts_ms:              u64,
}

impl SemanticArchitecture {
    pub fn fitness(&self) -> f32 {
        if !self.is_stable { return 0.0; }
        self.semantic_coherence * (1.0 - self.estimated_load)
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ArchStore {
    architectures: Vec<SemanticArchitecture>,
    seq:           u64,
}

impl ArchStore {
    fn new() -> Self { ArchStore { architectures: Vec::new(), seq: 0 } }
}

static STORE: Lazy<Mutex<ArchStore>> = Lazy::new(|| Mutex::new(ArchStore::new()));

// ── Generation logic ──────────────────────────────────────────────────────────

fn build_arch(store: &mut ArchStore, kind: ArchitectureKind) -> SemanticArchitecture {
    let unc      = crate::generalized_uncertainty::profile();
    let sem      = crate::semantic_stability::check();
    let avg_load = crate::adaptive_topology::avg_load();

    let semantic_coherence = match kind {
        ArchitectureKind::FlatSymbolic        => (1.0 - sem.instability_score * 1.2).clamp(0.0, 1.0),
        ArchitectureKind::HierarchicalSemantic=> (1.0 - unc.semantic_uncertainty).clamp(0.0, 1.0),
        ArchitectureKind::ProbabilisticMesh   => (1.0 - unc.overall * 0.8).clamp(0.0, 1.0),
        ArchitectureKind::AdaptiveHybrid      => {
            ((1.0 - sem.instability_score) * 0.5 + (1.0 - unc.overall) * 0.5).clamp(0.0, 1.0)
        }
    };

    let estimated_load = match kind {
        ArchitectureKind::FlatSymbolic         => avg_load * 0.60,
        ArchitectureKind::HierarchicalSemantic => avg_load * 1.20,
        ArchitectureKind::ProbabilisticMesh    => avg_load * 1.00,
        ArchitectureKind::AdaptiveHybrid       => avg_load * 0.90,
    }.clamp(0.0, 1.0);

    let depth = kind.base_depth();
    let is_stable = semantic_coherence > 0.40 && estimated_load < 0.75 && !sem.has_collapse_risk;

    store.seq += 1;
    let id = store.seq;

    ARCHITECTURES_GENERATED.fetch_add(1, Ordering::Relaxed);

    SemanticArchitecture { id, kind, depth, semantic_coherence, estimated_load, is_stable, ts_ms: ts_now() }
}

/// Generate the best architecture for current conditions.
pub fn generate_architecture() -> SemanticArchitecture {
    let unc = crate::generalized_uncertainty::profile();

    // Select kind based on current uncertainty regime
    let kind = if unc.overall < 0.30 {
        ArchitectureKind::FlatSymbolic
    } else if unc.semantic_uncertainty > 0.50 {
        ArchitectureKind::HierarchicalSemantic
    } else if unc.overall > 0.60 {
        ArchitectureKind::ProbabilisticMesh
    } else {
        ArchitectureKind::AdaptiveHybrid
    };

    let mut store = STORE.lock().unwrap();
    let arch = build_arch(&mut store, kind);
    if store.architectures.len() >= MAX_HISTORY { store.architectures.remove(0); }
    store.architectures.push(arch.clone());

    crate::world_evolution_observability::record(
        crate::world_evolution_observability::WorldSimEvent::WorldModelUpdated {
            component: arch.kind.label().into(),
            delta:     arch.semantic_coherence,
        }
    );

    arch
}

/// Generate all four architecture variants for comparison.
pub fn generate_all_variants() -> Vec<SemanticArchitecture> {
    let kinds = [
        ArchitectureKind::FlatSymbolic,
        ArchitectureKind::HierarchicalSemantic,
        ArchitectureKind::ProbabilisticMesh,
        ArchitectureKind::AdaptiveHybrid,
    ];
    let mut store = STORE.lock().unwrap();
    let mut variants = Vec::new();
    for kind in &kinds {
        let arch = build_arch(&mut store, *kind);
        if store.architectures.len() >= MAX_HISTORY { store.architectures.remove(0); }
        store.architectures.push(arch.clone());
        variants.push(arch);
    }
    variants
}

pub fn best_architecture() -> Option<SemanticArchitecture> {
    STORE.lock().unwrap().architectures.iter()
        .filter(|a| a.is_stable)
        .max_by(|a, b| a.fitness().partial_cmp(&b.fitness()).unwrap())
        .cloned()
}

pub fn recent(n: usize) -> Vec<SemanticArchitecture> {
    STORE.lock().unwrap().architectures.iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_architecture_no_panic() {
        let a = generate_architecture();
        assert!(a.semantic_coherence >= 0.0 && a.semantic_coherence <= 1.0);
        assert!(a.depth >= 1);
    }

    #[test]
    fn generate_all_variants_returns_four() {
        let v = generate_all_variants();
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn fitness_zero_when_unstable() {
        let a = SemanticArchitecture {
            id: 99, kind: ArchitectureKind::FlatSymbolic, depth: 1,
            semantic_coherence: 0.9, estimated_load: 0.9, is_stable: false,
            ts_ms: 0,
        };
        assert_eq!(a.fitness(), 0.0);
    }
}
