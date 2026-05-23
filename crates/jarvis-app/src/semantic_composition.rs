//! Semantic composition engine — composes semantic chains, merges conceptual
//! structures, and builds symbolic operational models from components.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static COMPOSITIONS_BUILT: AtomicU64 = AtomicU64::new(0);

const MAX_COMPOSITIONS: usize = 100;
const MAX_DEPTH:        usize = 4;

// ── ComposedStructure ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComposedStructure {
    pub id:          u64,
    pub root_label:  String,
    pub components:  Vec<String>,    // ordered leaf-to-root
    pub depth:       usize,
    pub coherence:   f32,            // 0–1; drops with more components
    pub ts_ms:       u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct CompositionState {
    compositions: Vec<ComposedStructure>,
    seq:          u64,
}

static STATE: Lazy<Mutex<CompositionState>> = Lazy::new(|| Mutex::new(CompositionState {
    compositions: Vec::new(),
    seq:          0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Composition logic ─────────────────────────────────────────────────────────

/// Compose a set of semantic structures into a hierarchical model.
/// Structures are ordered by confidence (lowest = leaf, highest = root).
pub fn compose(structure_labels: Vec<String>) -> Option<ComposedStructure> {
    if structure_labels.len() < 2 { return None; }
    if structure_labels.len() > MAX_DEPTH * 2 { return None; }

    // Find confidences from semantic graph
    let mut labeled: Vec<(String, f32)> = structure_labels.iter()
        .map(|label| {
            let conf = crate::semantic_graph::entity_id(label)
                .and_then(crate::semantic_graph::get_entity)
                .map(|e| e.confidence)
                .unwrap_or(0.35);
            (label.clone(), conf)
        })
        .collect();

    // Sort: lowest confidence first (leaves), highest last (root)
    labeled.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let components: Vec<String> = labeled.iter().map(|(l, _)| l.clone()).collect();
    let root_label = components.last()?.clone();

    // Coherence = product of confidences, decayed by component count
    let coherence = labeled.iter().map(|(_, c)| c)
        .fold(1.0f32, |acc, &c| acc * c.max(0.1))
        .powf(1.0 / labeled.len() as f32)   // geometric mean
        .clamp(0.0, 1.0);

    let depth = labeled.len().min(MAX_DEPTH);

    let cs = if let Ok(mut s) = STATE.lock() {
        if s.compositions.len() >= MAX_COMPOSITIONS { s.compositions.remove(0); }
        s.seq += 1;
        let id = s.seq;
        let c = ComposedStructure { id, root_label: root_label.clone(), components, depth, coherence, ts_ms: ts_now() };
        s.compositions.push(c.clone());
        c
    } else { return None; };

    // Wire Composed edges in semantic graph
    let root_id = crate::semantic_graph::add_entity(
        &root_label, crate::semantic_graph::EntityKind::Synthesis, coherence);
    for label in &cs.components {
        if label == &root_label { continue; }
        if let Some(src_id) = crate::semantic_graph::entity_id(label) {
            crate::semantic_graph::add_relation(
                src_id, root_id,
                crate::semantic_graph::SemanticRelation::Composed, coherence);
        }
    }

    COMPOSITIONS_BUILT.fetch_add(1, Ordering::Relaxed);
    Some(cs)
}

/// Compose from strong semantic structures automatically.
pub fn auto_compose() -> usize {
    let structs = crate::semantic_structures::strong_structures();
    let labels: Vec<String> = structs.iter().take(6).map(|s| s.label.clone()).collect();
    if labels.len() < 2 { return 0; }
    compose(labels).map(|_| 1).unwrap_or(0)
}

pub fn all_compositions() -> Vec<ComposedStructure> {
    STATE.lock().map(|s| s.compositions.clone()).unwrap_or_default()
}
