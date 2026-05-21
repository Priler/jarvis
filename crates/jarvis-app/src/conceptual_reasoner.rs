//! Conceptual reasoner — coordinates concept_engine, abstraction_graph, and
//! causal_reasoner to derive conceptual insights from operational events.
//! Feeds results into the abstraction graph and observability log.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static REASON_CYCLES: AtomicU64 = AtomicU64::new(0);

const MAX_RESULTS: usize = 100;

// ── ConceptualReasoningResult ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConceptualReasoningResult {
    pub cycle_id:          u64,
    pub concepts_active:   usize,
    pub new_edges:         usize,
    pub analogies_found:   usize,
    pub quality:           f32,    // 0–1
    pub dominant_kind:     String,
    pub recommendation:    String,
    pub ts_ms:             u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ReasonerState {
    results: Vec<ConceptualReasoningResult>,
}

static STATE: Lazy<Mutex<ReasonerState>> = Lazy::new(|| Mutex::new(ReasonerState {
    results: Vec::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Reasoning cycle ───────────────────────────────────────────────────────────

pub fn reason() -> ConceptualReasoningResult {
    let cycle_id = REASON_CYCLES.fetch_add(1, Ordering::Relaxed) + 1;
    let now = ts_now();

    // 1. Feed causal links into concept engine and graph
    let causal_links = crate::causal_reasoner::reliable_links();
    let mut new_edges = 0;
    for link in &causal_links {
        crate::concept_engine::observe(&link.cause);
        crate::concept_engine::observe(&link.effect);
        let from_id = crate::abstraction_graph::add_node(
            &link.cause, crate::abstraction_graph::NodeKind::Concept, link.strength);
        let to_id = crate::abstraction_graph::add_node(
            &link.effect, crate::abstraction_graph::NodeKind::Concept, link.strength);
        crate::abstraction_graph::add_edge(
            from_id, to_id,
            crate::abstraction_graph::EdgeKind::CausalLink,
            link.strength);
        new_edges += 1;
    }

    // 2. Feed workflow patterns into semantic structures and concept engine
    let patterns = crate::workflow_learning::strong_patterns();
    for p in &patterns {
        let label = p.sequence.join("_");
        crate::concept_engine::observe(&label);
        crate::semantic_structures::record(
            &label, crate::semantic_structures::StructureKind::RecurringWorkflow);
        let node_id = crate::abstraction_graph::add_node(
            &label, crate::abstraction_graph::NodeKind::Workflow,
            p.confidence);
        // Record SemanticSimilarity edges between workflow nodes
        let _ = node_id;
    }

    // 3. Feed failure patterns
    let failure_structures = crate::semantic_structures::by_kind(
        &crate::semantic_structures::StructureKind::RecurringFailure);
    for fs in &failure_structures {
        let node_id = crate::abstraction_graph::add_node(
            &fs.label, crate::abstraction_graph::NodeKind::Failure, fs.confidence);
        // Mark concepts with similar failure nodes as semantically similar
        let concepts_of_failure = crate::concept_engine::by_kind(
            &crate::concept_engine::ConceptKind::Failure);
        for fc in &concepts_of_failure {
            let cnode_id = crate::abstraction_graph::add_node(
                &fc.label, crate::abstraction_graph::NodeKind::Concept, fc.confidence);
            crate::abstraction_graph::add_edge(
                node_id, cnode_id,
                crate::abstraction_graph::EdgeKind::SemanticSimilarity,
                (fs.confidence + fc.confidence) / 2.0);
            new_edges += 1;
        }
    }

    // 4. Find analogies for the most confident failure concept
    let mut analogies_found = 0;
    let reliable = crate::concept_engine::reliable_concepts();
    if let Some(top) = reliable.iter().max_by(|a, b|
        a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
    {
        let mappings = crate::analogical_reasoner::find_analogies(&top.label);
        analogies_found = mappings.len();
        // Wire analogy edges into graph
        for m in &mappings {
            if m.validated {
                crate::abstraction_graph::link(
                    &m.source_label, crate::abstraction_graph::NodeKind::Concept,
                    &m.target_label, crate::abstraction_graph::NodeKind::Concept,
                    crate::abstraction_graph::EdgeKind::SemanticSimilarity,
                    m.similarity_score);
                new_edges += 1;
            }
        }
    }

    // 5. Compute quality: ratio of reliable concepts to total
    let total_concepts = crate::concept_engine::snapshot().len();
    let reliable_count = reliable.len();
    let quality = if total_concepts == 0 { 0.5 }
        else { reliable_count as f32 / total_concepts as f32 };

    // 6. Dominant kind
    let dominant_kind = {
        use crate::concept_engine::ConceptKind;
        let kinds = [
            ConceptKind::Failure, ConceptKind::Workflow, ConceptKind::Strategy,
            ConceptKind::Risk,    ConceptKind::Resource,
        ];
        kinds.iter()
            .map(|k| (k, crate::concept_engine::by_kind(k).len()))
            .max_by_key(|(_, c)| *c)
            .map(|(k, _)| format!("{:?}", k).to_lowercase())
            .unwrap_or_else(|| "unknown".to_string())
    };

    // 7. Recommendation
    let recommendation = if quality < 0.4 {
        "increase_observation_diversity".to_string()
    } else if analogies_found > 0 {
        "apply_analogical_transfer".to_string()
    } else {
        "continue_concept_accumulation".to_string()
    };

    // 8. Log quality to observability
    crate::conceptual_observability::log(
        crate::conceptual_observability::ConceptualEvent::GeneralizationQuality {
            quality, concept_count: total_concepts, strong_count: reliable_count,
        }
    );

    let result = ConceptualReasoningResult {
        cycle_id, concepts_active: reliable_count, new_edges,
        analogies_found, quality, dominant_kind, recommendation, ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.results.len() >= MAX_RESULTS { s.results.remove(0); }
        s.results.push(result.clone());
    }
    result
}

pub fn latest() -> Option<ConceptualReasoningResult> {
    STATE.lock().ok().and_then(|s| s.results.last().cloned())
}

pub fn history() -> Vec<ConceptualReasoningResult> {
    STATE.lock().map(|s| s.results.clone()).unwrap_or_default()
}
