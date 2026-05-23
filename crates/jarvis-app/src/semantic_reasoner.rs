//! Generalized semantic reasoner — coordinates all Phase 21 subsystems,
//! runs the full semantic reasoning cycle, and produces structured results.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static REASONING_CYCLES: AtomicU64 = AtomicU64::new(0);

const MAX_RESULTS: usize = 100;

// ── SemanticReasoningResult ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticReasoningResult {
    pub cycle_id:               u64,
    pub inference_chains_found: usize,
    pub contradictions_detected: usize,
    pub contradictions_resolved: usize,
    pub syntheses_created:      usize,
    pub constraint_violations:  usize,
    pub semantic_quality:       f32,    // 0–1
    pub world_state:            String,
    pub recommendation:         String,
    pub healthy:                bool,
    pub ts_ms:                  u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ReasonerState {
    results: Vec<SemanticReasoningResult>,
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

pub fn reason() -> SemanticReasoningResult {
    let cycle_id = REASONING_CYCLES.fetch_add(1, Ordering::Relaxed) + 1;
    let now = ts_now();

    // 1. Populate semantic graph from concepts and causal links
    let concepts = crate::concept_engine::reliable_concepts();
    for c in concepts.iter().take(20) {
        crate::semantic_graph::add_entity(&c.label, crate::semantic_graph::EntityKind::Concept, c.confidence);
    }
    let causal = crate::causal_reasoner::reliable_links();
    for link in causal.iter().take(15) {
        crate::semantic_graph::relate(
            &link.cause, crate::semantic_graph::EntityKind::Concept,
            &link.effect, crate::semantic_graph::EntityKind::Concept,
            crate::semantic_graph::SemanticRelation::Causal, link.strength,
        );
    }

    // 2. Run transitive inference in the graph
    crate::semantic_graph::infer_transitive();

    // 3. Forward inference from top 3 reliable concepts
    let mut inference_chains_found = 0;
    for c in concepts.iter().take(3) {
        if let Some(chain) = crate::symbolic_inference::forward_infer(&c.label) {
            if chain.is_reliable() { inference_chains_found += 1; }
            crate::symbolic_observability::log(
                crate::symbolic_observability::SymbolicEvent::InferenceChainBuilt {
                    root:       chain.root.clone(),
                    conclusion: chain.conclusion.clone(),
                    depth:      chain.steps.len(),
                    confidence: chain.confidence,
                }
            );
        }
    }

    // 4. Detect contradictions
    let new_contradictions = crate::semantic_contradictions::detect();
    let contradictions_detected = new_contradictions.len();
    for c in &new_contradictions {
        crate::symbolic_observability::log(
            crate::symbolic_observability::SymbolicEvent::ContradictionDetected {
                entity_a: c.entity_a.clone(),
                entity_b: c.entity_b.clone(),
                severity: c.severity,
            }
        );
    }

    // 5. Resolve contradictions (resolve oldest unresolved ones first)
    let unresolved = crate::semantic_contradictions::active_contradictions();
    let mut contradictions_resolved = 0;
    for c in unresolved.iter().take(3) {
        crate::semantic_contradictions::resolve(c.id);
        contradictions_resolved += 1;
        crate::symbolic_observability::log(
            crate::symbolic_observability::SymbolicEvent::ContradictionResolved {
                entity_a:   c.entity_a.clone(),
                entity_b:   c.entity_b.clone(),
                resolution: c.resolution.clone().unwrap_or_default(),
            }
        );
    }

    // 6. Cognitive synthesis from concept clusters
    let syntheses_created = crate::cognitive_synthesis::auto_synthesize();

    // 7. Semantic composition from strong structures
    crate::semantic_composition::auto_compose();

    // 8. Causal semantic analysis
    crate::abstract_causal_semantics::run_analysis();

    // 9. Check constraints
    let constraint_report = crate::constraint_reasoner::check_constraints();
    let constraint_violations = constraint_report.violated_count;

    // 10. Compute semantic quality
    let chains = crate::symbolic_inference::reliable_chains();
    let semantic_quality = if chains.is_empty() { 0.4 }
        else { chains.iter().map(|c| c.confidence).sum::<f32>() / chains.len() as f32 };

    // 11. World model update
    let world_snap = crate::symbolic_world_model::update();
    let world_state = format!("{:?}", world_snap.symbolic_state);

    // 12. Recommendation
    let recommendation = if contradictions_detected > crate::symbolic_safety::MAX_CONTRADICTIONS_PER_CYCLE {
        "resolve_contradiction_cascade".to_string()
    } else if constraint_report.has_critical_violation() {
        "address_critical_constraint".to_string()
    } else if inference_chains_found == 0 && concepts.len() > 3 {
        "enrich_semantic_graph".to_string()
    } else {
        "continue_semantic_reasoning".to_string()
    };

    let healthy = semantic_quality >= 0.25
        && contradictions_detected < crate::symbolic_safety::MAX_CONTRADICTIONS_PER_CYCLE
        && !constraint_report.has_critical_violation();

    let result = SemanticReasoningResult {
        cycle_id, inference_chains_found, contradictions_detected,
        contradictions_resolved, syntheses_created, constraint_violations,
        semantic_quality, world_state, recommendation, healthy, ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.results.len() >= MAX_RESULTS { s.results.remove(0); }
        s.results.push(result.clone());
    }
    result
}

pub fn latest() -> Option<SemanticReasoningResult> {
    STATE.lock().ok().and_then(|s| s.results.last().cloned())
}

pub fn history() -> Vec<SemanticReasoningResult> {
    STATE.lock().map(|s| s.results.clone()).unwrap_or_default()
}
