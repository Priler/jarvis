//! Symbolic inference engine — forward/backward inference, chain reasoning,
//! semantic implication detection, and constraint propagation.
//! Chains are depth-limited (MAX_CHAIN_DEPTH = 8) and confidence-decayed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static CHAINS_BUILT:     AtomicU64 = AtomicU64::new(0);
pub static STEPS_INFERRED:   AtomicU64 = AtomicU64::new(0);
pub static CHAINS_REJECTED:  AtomicU64 = AtomicU64::new(0);

const MAX_CHAIN_DEPTH:   usize = 8;
const MAX_CHAINS_STORED: usize = 200;

// ── Inference step / chain ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceStep {
    pub from_label:  String,
    pub to_label:    String,
    pub relation:    String,
    pub step_confidence: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum InferenceKind { Forward, Backward, ConstraintPropagation, DependencyReasoning }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferenceChain {
    pub id:         u64,
    pub kind:       InferenceKind,
    pub root:       String,
    pub conclusion: String,
    pub steps:      Vec<InferenceStep>,
    pub confidence: f32,    // product of step confidences
    pub depth:      usize,
    pub valid:      bool,
    pub ts_ms:      u64,
}

impl InferenceChain {
    pub fn is_reliable(&self) -> bool {
        self.valid && self.confidence >= crate::symbolic_safety::MIN_CHAIN_CONFIDENCE
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct InferenceState {
    chains: Vec<InferenceChain>,
    seq:    u64,
}

static STATE: Lazy<Mutex<InferenceState>> = Lazy::new(|| Mutex::new(InferenceState {
    chains: Vec::new(),
    seq:    0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn relation_label(r: &crate::semantic_graph::SemanticRelation) -> String {
    format!("{:?}", r).to_lowercase()
}

// ── Forward inference ─────────────────────────────────────────────────────────

/// Build a forward inference chain starting from a given entity label.
/// Follows Implies, Causal, Inferred edges, halts at Contradicts or max depth.
pub fn forward_infer(start_label: &str) -> Option<InferenceChain> {
    let start_id = crate::semantic_graph::entity_id(start_label)?;

    let mut steps    = Vec::new();
    let mut current  = start_id;
    let mut conf     = crate::semantic_graph::get_entity(start_id)?.confidence;
    let mut conclusion = start_label.to_string();
    let mut visited  = std::collections::HashSet::new();
    visited.insert(start_id);

    for depth in 0..MAX_CHAIN_DEPTH {
        let edges = crate::semantic_graph::outgoing(current);
        // Pick the highest-weight non-contradiction, non-visited edge
        let best = edges.iter()
            .filter(|e| !e.is_contradiction() && !visited.contains(&e.to))
            .max_by(|a, b| a.weight.partial_cmp(&b.weight).unwrap_or(std::cmp::Ordering::Equal));

        let edge = match best { Some(e) => e.clone(), None => break };
        let decay = edge.relation.confidence_factor();
        let step_conf = edge.weight * decay;
        conf *= step_conf;

        let to_entity = match crate::semantic_graph::get_entity(edge.to) {
            Some(e) => e, None => break,
        };
        steps.push(InferenceStep {
            from_label:      conclusion.clone(),
            to_label:        to_entity.label.clone(),
            relation:        relation_label(&edge.relation),
            step_confidence: step_conf,
        });
        conclusion = to_entity.label.clone();
        visited.insert(edge.to);
        current = edge.to;
        STEPS_INFERRED.fetch_add(1, Ordering::Relaxed);

        // Safety check at each step
        let verdict = crate::symbolic_safety::validate_chain(depth + 1, conf);
        if !verdict.is_valid() {
            CHAINS_REJECTED.fetch_add(1, Ordering::Relaxed);
            break;
        }
    }

    if steps.is_empty() { return None; }

    let valid = crate::symbolic_safety::validate_chain(steps.len(), conf).is_valid();
    let chain_id = STATE.lock().map(|mut s| { s.seq += 1; s.seq }).unwrap_or(0);

    let chain = InferenceChain {
        id: chain_id,
        kind: InferenceKind::Forward,
        root: start_label.to_string(),
        conclusion,
        steps,
        confidence: conf,
        depth: chain_id as usize % MAX_CHAIN_DEPTH, // bounded display
        valid,
        ts_ms: ts_now(),
    };

    store_chain(chain.clone());
    if valid { CHAINS_BUILT.fetch_add(1, Ordering::Relaxed); }
    Some(chain)
}

// ── Backward inference ────────────────────────────────────────────────────────

/// Find the most likely antecedents (roots) that imply a given target.
pub fn backward_infer(target_label: &str) -> Vec<InferenceChain> {
    let target_id = match crate::semantic_graph::entity_id(target_label) {
        Some(id) => id, None => return Vec::new(),
    };

    let all_edges = crate::semantic_graph::snapshot_edges();
    let antecedent_ids: Vec<u64> = all_edges.iter()
        .filter(|e| e.to == target_id && !e.is_contradiction())
        .map(|e| e.from)
        .collect();

    antecedent_ids.iter()
        .filter_map(|&root_id| {
            let root_entity = crate::semantic_graph::get_entity(root_id)?;
            forward_infer(&root_entity.label)
                .filter(|c| c.conclusion == target_label)
        })
        .collect()
}

// ── Constraint propagation ────────────────────────────────────────────────────

/// Build chains from all constraint entities through their Constrains edges.
pub fn propagate_constraints() -> Vec<InferenceChain> {
    let entities = crate::semantic_graph::snapshot_entities();
    entities.iter()
        .filter(|e| e.kind == crate::semantic_graph::EntityKind::Constraint)
        .filter_map(|e| forward_infer(&e.label))
        .filter(|c| c.is_reliable())
        .collect()
}

// ── Storage ───────────────────────────────────────────────────────────────────

fn store_chain(chain: InferenceChain) {
    if let Ok(mut s) = STATE.lock() {
        if s.chains.len() >= MAX_CHAINS_STORED { s.chains.remove(0); }
        s.chains.push(chain);
    }
}

pub fn reliable_chains() -> Vec<InferenceChain> {
    STATE.lock()
        .map(|s| s.chains.iter().filter(|c| c.is_reliable()).cloned().collect())
        .unwrap_or_default()
}

pub fn recent_chains(n: usize) -> Vec<InferenceChain> {
    STATE.lock()
        .map(|s| s.chains.iter().rev().take(n).cloned().collect())
        .unwrap_or_default()
}

pub fn all_chains() -> Vec<InferenceChain> {
    STATE.lock().map(|s| s.chains.clone()).unwrap_or_default()
}
