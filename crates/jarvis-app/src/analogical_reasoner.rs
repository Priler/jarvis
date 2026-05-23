//! Analogical reasoning engine — detects structural similarity between concepts
//! and transfers solutions/strategies across domains.
//! Transfer is validated by conceptual_safety before it is applied.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static ANALOGIES_FOUND:    AtomicU64 = AtomicU64::new(0);
pub static TRANSFERS_ATTEMPTED: AtomicU64 = AtomicU64::new(0);
pub static TRANSFERS_VALID:    AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY:         usize = 200;
const MIN_SIMILARITY:      f32   = 0.55;  // minimum structural similarity for analogy
const MIN_TRANSFER_CONF:   f32   = 0.50;  // minimum confidence for transfer

// ── AnalogicalMapping ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalogicalMapping {
    pub source_label:       String,
    pub target_label:       String,
    pub similarity_score:   f32,
    pub transferred_strategy: String,
    pub confidence:         f32,
    pub validated:          bool,
    pub ts_ms:              u64,
}

// ── TransferResult ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransferResult {
    pub from:      String,
    pub to:        String,
    pub strategy:  String,
    pub success:   bool,
    pub reason:    String,
    pub ts_ms:     u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct AnalogyState {
    mappings: Vec<AnalogicalMapping>,
    results:  Vec<TransferResult>,
}

static STATE: Lazy<Mutex<AnalogyState>> = Lazy::new(|| Mutex::new(AnalogyState {
    mappings: Vec::new(),
    results:  Vec::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Structural similarity heuristic ──────────────────────────────────────────

/// Compute structural similarity between two concept labels.
/// Words in common / max(len) — simple Jaccard-style token overlap.
fn structural_similarity(a: &str, b: &str) -> f32 {
    let tokens_a: std::collections::HashSet<&str> =
        a.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
    let tokens_b: std::collections::HashSet<&str> =
        b.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
    if tokens_a.is_empty() && tokens_b.is_empty() { return 1.0; }
    let intersection = tokens_a.intersection(&tokens_b).count() as f32;
    let union        = tokens_a.union(&tokens_b).count() as f32;
    if union == 0.0 { 0.0 } else { intersection / union }
}

/// Derive a transfer strategy from two analogous concepts.
fn derive_strategy(source: &str, target: &str) -> String {
    let src = source.to_lowercase();
    let tgt = target.to_lowercase();
    if src.contains("instab") || tgt.contains("instab") {
        "apply_stabilization_strategy".to_string()
    } else if src.contains("starv") || tgt.contains("starv") {
        "apply_resource_rebalancing".to_string()
    } else if src.contains("failure") || tgt.contains("failure") {
        "apply_recovery_and_retry".to_string()
    } else if src.contains("optim") || tgt.contains("optim") {
        "apply_optimization_transfer".to_string()
    } else {
        "apply_generalized_mitigation".to_string()
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Find analogical mappings for a given concept label from the graph of known concepts.
/// Compares against all concepts in the concept_engine.
pub fn find_analogies(label: &str) -> Vec<AnalogicalMapping> {
    let now = ts_now();
    let concepts = crate::concept_engine::reliable_concepts();
    let mut found = Vec::new();

    for concept in &concepts {
        if concept.label == label { continue; }
        let sim = structural_similarity(label, &concept.label);
        if sim >= MIN_SIMILARITY {
            let strategy = derive_strategy(label, &concept.label);
            let confidence = (sim * concept.confidence).min(1.0);
            let validated  = crate::conceptual_safety::validate_transfer(label, &concept.label, sim);
            found.push(AnalogicalMapping {
                source_label:       label.to_string(),
                target_label:       concept.label.clone(),
                similarity_score:   sim,
                transferred_strategy: strategy,
                confidence,
                validated,
                ts_ms: now,
            });
            ANALOGIES_FOUND.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Also check abstraction graph for SemanticSimilarity edges
    if let Some(src_id) = crate::abstraction_graph::node_id(label) {
        let similar = crate::abstraction_graph::similar_nodes(src_id);
        for node in similar {
            if found.iter().any(|m| m.target_label == node.label) { continue; }
            let sim = node.weight.max(MIN_SIMILARITY);
            let strategy = derive_strategy(label, &node.label);
            let confidence = sim;
            let validated  = crate::conceptual_safety::validate_transfer(label, &node.label, sim);
            found.push(AnalogicalMapping {
                source_label:       label.to_string(),
                target_label:       node.label.clone(),
                similarity_score:   sim,
                transferred_strategy: strategy,
                confidence,
                validated,
                ts_ms: now,
            });
            ANALOGIES_FOUND.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Store in history
    if let Ok(mut s) = STATE.lock() {
        for m in &found {
            if s.mappings.len() >= MAX_HISTORY { s.mappings.remove(0); }
            s.mappings.push(m.clone());
        }
    }

    found
}

/// Attempt to apply an analogical transfer from source to target.
pub fn apply_analog(source: &str, target: &str) -> Option<TransferResult> {
    TRANSFERS_ATTEMPTED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let sim = structural_similarity(source, target);
    if sim < MIN_SIMILARITY {
        let result = TransferResult {
            from: source.to_string(), to: target.to_string(),
            strategy: String::new(), success: false,
            reason: format!("similarity_too_low:{:.2}", sim), ts_ms: now,
        };
        if let Ok(mut s) = STATE.lock() {
            if s.results.len() >= MAX_HISTORY { s.results.remove(0); }
            s.results.push(result.clone());
        }
        return Some(result);
    }

    let validated = crate::conceptual_safety::validate_transfer(source, target, sim);
    if !validated {
        let result = TransferResult {
            from: source.to_string(), to: target.to_string(),
            strategy: String::new(), success: false,
            reason: "safety_rejected".to_string(), ts_ms: now,
        };
        if let Ok(mut s) = STATE.lock() {
            if s.results.len() >= MAX_HISTORY { s.results.remove(0); }
            s.results.push(result.clone());
        }
        return Some(result);
    }

    // Check concept confidence
    let source_concept = crate::concept_engine::best_match(source);
    let confidence = source_concept.map(|c| c.confidence).unwrap_or(0.4);
    if confidence < MIN_TRANSFER_CONF {
        let result = TransferResult {
            from: source.to_string(), to: target.to_string(),
            strategy: String::new(), success: false,
            reason: format!("source_confidence_too_low:{:.2}", confidence), ts_ms: now,
        };
        if let Ok(mut s) = STATE.lock() {
            if s.results.len() >= MAX_HISTORY { s.results.remove(0); }
            s.results.push(result.clone());
        }
        return Some(result);
    }

    let strategy = derive_strategy(source, target);
    TRANSFERS_VALID.fetch_add(1, Ordering::Relaxed);

    // Register transfer in graph
    crate::abstraction_graph::link(
        source, crate::abstraction_graph::NodeKind::Concept,
        target, crate::abstraction_graph::NodeKind::Concept,
        crate::abstraction_graph::EdgeKind::StrategicTransfer, sim,
    );

    let result = TransferResult {
        from: source.to_string(), to: target.to_string(),
        strategy, success: true, reason: "validated".to_string(), ts_ms: now,
    };
    if let Ok(mut s) = STATE.lock() {
        if s.results.len() >= MAX_HISTORY { s.results.remove(0); }
        s.results.push(result.clone());
    }
    Some(result)
}

pub fn recent_mappings(n: usize) -> Vec<AnalogicalMapping> {
    STATE.lock().map(|s| s.mappings.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

pub fn recent_results(n: usize) -> Vec<TransferResult> {
    STATE.lock().map(|s| s.results.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}
