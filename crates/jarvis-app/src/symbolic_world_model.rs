//! Symbolic world model — understands symbolic operational state, semantic
//! dependencies, conceptual causality, and abstract workflow evolution.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

// ── SymbolicState ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SymbolicState {
    Coherent,               // all beliefs consistent, inference productive
    Fragmentary,            // some entities without connections
    Contradictory,          // active contradictions in semantic graph
    Degrading { rate: f32 },// inference quality declining
    Synthesizing,           // active concept synthesis in progress
    Unknown,
}

// ── SymbolicWorldSnapshot ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolicWorldSnapshot {
    pub symbolic_state:        SymbolicState,
    pub entity_count:          usize,
    pub edge_count:            usize,
    pub chain_count:           usize,
    pub active_contradictions: usize,
    pub synthesis_count:       usize,
    pub inference_quality:     f32,    // 0–1
    pub semantic_coherence:    f32,    // 0–1
    pub ts_ms:                 u64,
}

impl SymbolicWorldSnapshot {
    pub fn is_coherent(&self) -> bool { self.symbolic_state == SymbolicState::Coherent }
    pub fn needs_resolution(&self) -> bool { self.active_contradictions > 0 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct WorldState {
    history: Vec<SymbolicWorldSnapshot>,
}

static STATE: Lazy<Mutex<WorldState>> = Lazy::new(|| Mutex::new(WorldState {
    history: Vec::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Update ────────────────────────────────────────────────────────────────────

pub fn update() -> SymbolicWorldSnapshot {
    let (entity_count, edge_count) = crate::semantic_graph::stats();
    let chains = crate::symbolic_inference::reliable_chains();
    let chain_count = chains.len();
    let contradictions = crate::semantic_contradictions::active_contradictions();
    let active_contradictions = contradictions.len();
    let syntheses = crate::cognitive_synthesis::all_syntheses();
    let synthesis_count = syntheses.len();

    // Inference quality: average confidence of reliable chains
    let inference_quality = if chains.is_empty() { 0.5 }
        else { chains.iter().map(|c| c.confidence).sum::<f32>() / chains.len() as f32 };

    // Semantic coherence: fraction of entities with at least one edge
    let entities = crate::semantic_graph::snapshot_entities();
    let edges = crate::semantic_graph::snapshot_edges();
    let connected = entities.iter().filter(|e| {
        edges.iter().any(|ed| ed.from == e.id || ed.to == e.id)
    }).count();
    let semantic_coherence = if entities.is_empty() { 1.0 }
        else { connected as f32 / entities.len() as f32 };

    // Symbolic state derivation
    let symbolic_state = if active_contradictions > 3 {
        SymbolicState::Contradictory
    } else if synthesis_count > 0 && active_contradictions == 0 {
        SymbolicState::Synthesizing
    } else if inference_quality < 0.30 && chain_count > 0 {
        SymbolicState::Degrading { rate: 1.0 - inference_quality }
    } else if semantic_coherence < 0.40 {
        SymbolicState::Fragmentary
    } else if active_contradictions == 0 && inference_quality >= 0.50 {
        SymbolicState::Coherent
    } else {
        SymbolicState::Unknown
    };

    let snap = SymbolicWorldSnapshot {
        symbolic_state, entity_count, edge_count, chain_count,
        active_contradictions, synthesis_count, inference_quality,
        semantic_coherence, ts_ms: ts_now(),
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(snap.clone());
    }
    snap
}

pub fn latest() -> Option<SymbolicWorldSnapshot> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn history() -> Vec<SymbolicWorldSnapshot> {
    STATE.lock().map(|s| s.history.clone()).unwrap_or_default()
}
