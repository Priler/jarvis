//! Semantic graph — symbolic entities connected by typed semantic relations.
//! Supports inference edges, contradiction edges, causal semantic edges,
//! and conceptual inheritance. Foundation for all Phase 21 symbolic reasoning.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static ENTITIES_ADDED:   AtomicU64 = AtomicU64::new(0);
pub static RELATIONS_ADDED:  AtomicU64 = AtomicU64::new(0);
pub static INFERENCES_MADE:  AtomicU64 = AtomicU64::new(0);

const MAX_ENTITIES:  usize = 600;
const MAX_RELATIONS: usize = 3000;

// ── Entity kind ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EntityKind {
    Concept,
    Workflow,
    Resource,
    State,
    Goal,
    Constraint,
    Strategy,
    Inference,    // a derived conclusion
    Synthesis,    // a synthesized concept
}

// ── Semantic relation ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SemanticRelation {
    Causal,        // A causes B
    Implies,       // A → B (logical implication)
    Contradicts,   // A and B are mutually exclusive
    Inherits,      // A is a subtype of B
    Composed,      // A is composed of B
    Constrains,    // A limits B
    Mitigates,     // A reduces B
    Equivalent,    // A ≡ B
    Inferred,      // B was inferred from A
}

impl SemanticRelation {
    /// Confidence decay applied when traversing this edge type.
    pub fn confidence_factor(&self) -> f32 {
        match self {
            SemanticRelation::Causal      => 0.90,
            SemanticRelation::Implies     => 0.85,
            SemanticRelation::Inferred    => 0.80,
            SemanticRelation::Inherits    => 0.95,
            SemanticRelation::Composed    => 0.90,
            SemanticRelation::Constrains  => 0.85,
            SemanticRelation::Mitigates   => 0.80,
            SemanticRelation::Equivalent  => 1.00,
            SemanticRelation::Contradicts => 0.00, // contradiction halts propagation
        }
    }
}

// ── Entity / edge ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticEntity {
    pub id:         u64,
    pub kind:       EntityKind,
    pub label:      String,
    pub confidence: f32,    // 0–1
    pub ts_ms:      u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticEdge {
    pub id:       u64,
    pub from:     u64,
    pub to:       u64,
    pub relation: SemanticRelation,
    pub weight:   f32,
    pub ts_ms:    u64,
}

impl SemanticEdge {
    pub fn is_contradiction(&self) -> bool {
        self.relation == SemanticRelation::Contradicts
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct GraphState {
    entities:  Vec<SemanticEntity>,
    edges:     Vec<SemanticEdge>,
    entity_seq: u64,
    edge_seq:   u64,
    label_idx:  HashMap<String, u64>,
}

static STATE: Lazy<Mutex<GraphState>> = Lazy::new(|| Mutex::new(GraphState {
    entities:   Vec::new(),
    edges:      Vec::new(),
    entity_seq: 0,
    edge_seq:   0,
    label_idx:  HashMap::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Add or reinforce a semantic entity. Returns its id.
pub fn add_entity(label: impl Into<String>, kind: EntityKind, confidence: f32) -> u64 {
    let label = label.into();
    let now = ts_now();
    if let Ok(mut s) = STATE.lock() {
        if let Some(&id) = s.label_idx.get(&label) {
            if let Some(e) = s.entities.iter_mut().find(|e| e.id == id) {
                e.confidence = (e.confidence + 0.05).min(1.0);
                e.ts_ms = now;
            }
            return id;
        }
        if s.entities.len() >= MAX_ENTITIES { s.entities.remove(0); }
        s.entity_seq += 1;
        let id = s.entity_seq;
        s.label_idx.insert(label.clone(), id);
        s.entities.push(SemanticEntity {
            id, kind, label, confidence: confidence.clamp(0.0, 1.0), ts_ms: now,
        });
        ENTITIES_ADDED.fetch_add(1, Ordering::Relaxed);
        id
    } else { 0 }
}

/// Add a semantic relation (edge). Duplicate relation+direction is reinforced.
pub fn add_relation(from: u64, to: u64, relation: SemanticRelation, weight: f32) {
    if from == to { return; }
    let now = ts_now();
    if let Ok(mut s) = STATE.lock() {
        if let Some(e) = s.edges.iter_mut()
            .find(|e| e.from == from && e.to == to && e.relation == relation)
        {
            e.weight = (e.weight + 0.05).min(1.0);
            e.ts_ms  = now;
            return;
        }
        if s.edges.len() >= MAX_RELATIONS { s.edges.remove(0); }
        s.edge_seq += 1;
        let eid = s.edge_seq;
        s.edges.push(SemanticEdge {
            id: eid, from, to, relation, weight: weight.clamp(0.0, 1.0), ts_ms: now,
        });
        RELATIONS_ADDED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Add a relation by label (auto-creates entities if needed).
pub fn relate(
    from_label: impl Into<String>, from_kind: EntityKind,
    to_label: impl Into<String>,   to_kind:   EntityKind,
    relation: SemanticRelation, weight: f32,
) {
    let from_id = add_entity(from_label, from_kind, weight);
    let to_id   = add_entity(to_label,   to_kind,   weight);
    add_relation(from_id, to_id, relation, weight);
}

/// Entity id by label.
pub fn entity_id(label: &str) -> Option<u64> {
    STATE.lock().ok().and_then(|s| s.label_idx.get(label).copied())
}

/// Get entity by id.
pub fn get_entity(id: u64) -> Option<SemanticEntity> {
    STATE.lock().ok().and_then(|s| s.entities.iter().find(|e| e.id == id).cloned())
}

/// Outgoing edges from a given entity.
pub fn outgoing(entity_id: u64) -> Vec<SemanticEdge> {
    STATE.lock()
        .map(|s| s.edges.iter().filter(|e| e.from == entity_id).cloned().collect())
        .unwrap_or_default()
}

/// All contradiction edges in the graph.
pub fn contradiction_edges() -> Vec<SemanticEdge> {
    STATE.lock()
        .map(|s| s.edges.iter().filter(|e| e.is_contradiction()).cloned().collect())
        .unwrap_or_default()
}

/// Infer transitive relations: if A→B and B→C (same non-contradiction relation), add A→C.
/// Bounded to one pass to avoid recursive explosion.
pub fn infer_transitive() {
    let (entities_snap, edges_snap) = STATE.lock()
        .map(|s| (s.entities.clone(), s.edges.clone()))
        .unwrap_or_default();

    let mut new_edges = Vec::new();
    for e1 in &edges_snap {
        if e1.relation == SemanticRelation::Contradicts { continue; }
        for e2 in &edges_snap {
            if e2.from != e1.to { continue; }
            if e2.relation == SemanticRelation::Contradicts { continue; }
            if e1.from == e2.to { continue; } // cycle guard
            // Check that this transitive edge doesn't already exist
            if edges_snap.iter().any(|e| e.from == e1.from && e.to == e2.to
                && e.relation == SemanticRelation::Inferred) { continue; }
            let weight = e1.weight * e2.weight * 0.80;
            if weight < 0.20 { continue; }
            // Verify both entities still exist
            if entities_snap.iter().any(|en| en.id == e1.from)
            && entities_snap.iter().any(|en| en.id == e2.to) {
                new_edges.push((e1.from, e2.to, weight));
                INFERENCES_MADE.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    for (from, to, weight) in new_edges {
        add_relation(from, to, SemanticRelation::Inferred, weight);
    }
}

pub fn snapshot_entities() -> Vec<SemanticEntity> {
    STATE.lock().map(|s| s.entities.clone()).unwrap_or_default()
}

pub fn snapshot_edges() -> Vec<SemanticEdge> {
    STATE.lock().map(|s| s.edges.clone()).unwrap_or_default()
}

pub fn stats() -> (usize, usize) {
    STATE.lock().map(|s| (s.entities.len(), s.edges.len())).unwrap_or((0, 0))
}
