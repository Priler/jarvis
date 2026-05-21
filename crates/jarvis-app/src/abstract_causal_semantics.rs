//! Abstract causal semantics — combines causal_reasoner with semantic_graph
//! to build abstract causal hierarchies and infer semantic propagation.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_CHAINS: usize = 100;

// ── CausalSemanticChain ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CausalSemanticChain {
    pub root_cause:      String,
    pub semantic_path:   Vec<String>,   // semantic entities from root to conclusion
    pub causal_strength: f32,
    pub semantic_conf:   f32,
    pub combined_score:  f32,
    pub ts_ms:           u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct CausalSemState {
    chains: Vec<CausalSemanticChain>,
}

static STATE: Lazy<Mutex<CausalSemState>> = Lazy::new(|| Mutex::new(CausalSemState {
    chains: Vec::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Build a causal-semantic chain starting from a given cause label.
/// Combines the causal strength from causal_reasoner with a semantic
/// inference chain from the semantic_graph.
pub fn build_chain(cause: &str) -> Option<CausalSemanticChain> {
    // Get causal strength from causal_reasoner
    let causal_links = crate::causal_reasoner::reliable_links();
    let causal_strength = causal_links.iter()
        .filter(|l| l.cause == cause)
        .map(|l| l.strength)
        .fold(0.0f32, f32::max);

    // Register the cause in semantic graph and run forward inference
    crate::semantic_graph::add_entity(
        cause, crate::semantic_graph::EntityKind::Concept, causal_strength.max(0.4));

    let chain = crate::symbolic_inference::forward_infer(cause)?;
    let semantic_conf = chain.confidence;

    let semantic_path: Vec<String> = std::iter::once(chain.root.clone())
        .chain(chain.steps.iter().map(|s| s.to_label.clone()))
        .collect();

    let combined_score = (causal_strength * 0.5 + semantic_conf * 0.5).clamp(0.0, 1.0);

    let cs = CausalSemanticChain {
        root_cause: cause.to_string(),
        semantic_path, causal_strength, semantic_conf, combined_score,
        ts_ms: ts_now(),
    };

    if let Ok(mut s) = STATE.lock() {
        if s.chains.len() >= MAX_CHAINS { s.chains.remove(0); }
        s.chains.push(cs.clone());
    }
    Some(cs)
}

/// Infer semantic propagation: what concepts will be affected if `root_cause` occurs?
pub fn infer_semantic_propagation(root_cause: &str) -> Vec<crate::semantic_graph::SemanticEntity> {
    // Register in graph
    crate::semantic_graph::add_entity(
        root_cause, crate::semantic_graph::EntityKind::Concept, 0.5);

    // Build inference chain
    let chain = match crate::symbolic_inference::forward_infer(root_cause) {
        Some(c) => c, None => return Vec::new(),
    };

    chain.steps.iter()
        .filter_map(|step| {
            crate::semantic_graph::entity_id(&step.to_label)
                .and_then(crate::semantic_graph::get_entity)
        })
        .collect()
}

/// Run causal-semantic analysis on all reliable causal links.
pub fn run_analysis() -> usize {
    let links = crate::causal_reasoner::reliable_links();
    let mut built = 0;
    for link in links.iter().take(10) {
        if build_chain(&link.cause).is_some() { built += 1; }
    }
    built
}

pub fn all_chains() -> Vec<CausalSemanticChain> {
    STATE.lock().map(|s| s.chains.clone()).unwrap_or_default()
}

pub fn latest() -> Option<CausalSemanticChain> {
    STATE.lock().ok().and_then(|s| s.chains.last().cloned())
}
