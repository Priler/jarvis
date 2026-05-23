//! Cognitive synthesis engine — merges abstractions, synthesizes strategies,
//! composes generalized concepts, generates semantic operational models.
//! All synthesis validated by symbolic_safety before propagation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static SYNTHESES_ATTEMPTED: AtomicU64 = AtomicU64::new(0);
pub static SYNTHESES_CREATED:   AtomicU64 = AtomicU64::new(0);

const MAX_SYNTHESES: usize = 100;

// ── SynthesizedConcept ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SynthesizedConcept {
    pub id:               u64,
    pub label:            String,
    pub source_entities:  Vec<String>,
    pub confidence:       f32,
    pub abstraction_level: u8,
    pub ts_ms:            u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SynthesisState {
    syntheses: Vec<SynthesizedConcept>,
    seq:       u64,
}

static STATE: Lazy<Mutex<SynthesisState>> = Lazy::new(|| Mutex::new(SynthesisState {
    syntheses: Vec::new(),
    seq:       0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Synthesis patterns ────────────────────────────────────────────────────────

/// Attempt to synthesize a named concept from a combination of entity labels.
fn match_synthesis_pattern(labels: &[String]) -> Option<(&'static str, u8)> {
    let joined = labels.join(" ");
    let j = joined.to_lowercase();

    // Known synthesis patterns: specific combinations → synthesized label + abstraction level
    let patterns: &[(&[&str], &str, u8)] = &[
        (&["scheduler", "resource", "planner"],   "systemic_cognition_degradation",     3),
        (&["scheduler", "instability", "overload"], "systemic_cognition_degradation",   3),
        (&["instability", "overload", "uncertainty"], "systemic_cognition_degradation", 3),
        (&["failure", "recovery", "instability"], "recursive_failure_pattern",          2),
        (&["starvation", "bottleneck"],            "resource_exhaustion_syndrome",       2),
        (&["drift", "oscillation"],                "cognitive_drift_syndrome",            2),
        (&["uncertainty", "risk"],                 "epistemic_fragility",                2),
        (&["contradiction", "conflict"],           "semantic_inconsistency",             2),
        (&["optimize", "constrain"],               "constrained_optimization",           2),
        (&["goal", "constraint", "resource"],      "resource_constrained_planning",      2),
    ];

    for (keywords, label, level) in patterns {
        if keywords.iter().all(|kw| j.contains(kw)) {
            return Some((label, *level));
        }
    }
    None
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Synthesize a concept from a set of entity labels.
/// Returns Some if a synthesis pattern matches and safety passes.
pub fn synthesize(entity_labels: Vec<String>) -> Option<SynthesizedConcept> {
    SYNTHESES_ATTEMPTED.fetch_add(1, Ordering::Relaxed);

    if entity_labels.len() < 2 { return None; }

    let (syn_label, abs_level) = match_synthesis_pattern(&entity_labels)?;

    // Compute combined confidence from source entities
    let confidences: Vec<f32> = entity_labels.iter()
        .filter_map(|label| crate::semantic_graph::entity_id(label))
        .filter_map(crate::semantic_graph::get_entity)
        .map(|e| e.confidence)
        .collect();
    let confidence = if confidences.is_empty() { 0.35 }
        else { confidences.iter().sum::<f32>() / confidences.len() as f32 * 0.85 };

    // Safety validation
    let verdict = crate::symbolic_safety::validate_synthesis(entity_labels.len(), confidence);
    if !verdict.is_valid() { return None; }

    // Check if this synthesis already exists
    let already = STATE.lock()
        .map(|s| s.syntheses.iter().any(|c| c.label == syn_label))
        .unwrap_or(false);

    let concept = if let Ok(mut s) = STATE.lock() {
        if already {
            // Reinforce existing
            if let Some(existing) = s.syntheses.iter_mut().find(|c| c.label == syn_label) {
                existing.confidence = (existing.confidence + 0.05).min(1.0);
                existing.ts_ms = ts_now();
            }
            return s.syntheses.iter().find(|c| c.label == syn_label).cloned();
        }
        if s.syntheses.len() >= MAX_SYNTHESES { s.syntheses.remove(0); }
        s.seq += 1;
        let id = s.seq;
        let c = SynthesizedConcept {
            id, label: syn_label.to_string(),
            source_entities: entity_labels.clone(),
            confidence, abstraction_level: abs_level,
            ts_ms: ts_now(),
        };
        s.syntheses.push(c.clone());
        c
    } else { return None; };

    // Register synthesized concept in semantic graph
    let eid = crate::semantic_graph::add_entity(
        &concept.label, crate::semantic_graph::EntityKind::Synthesis, confidence);
    for label in &entity_labels {
        if let Some(src_id) = crate::semantic_graph::entity_id(label) {
            crate::semantic_graph::add_relation(
                src_id, eid,
                crate::semantic_graph::SemanticRelation::Composed,
                confidence);
        }
    }

    SYNTHESES_CREATED.fetch_add(1, Ordering::Relaxed);

    // Log to observability
    crate::symbolic_observability::log(
        crate::symbolic_observability::SymbolicEvent::ConceptSynthesized {
            label: concept.label.clone(),
            sources: entity_labels,
            confidence,
        }
    );

    Some(concept)
}

/// Attempt synthesis from all combinations of reliable concepts (top 5 by confidence).
pub fn auto_synthesize() -> usize {
    let concepts = crate::concept_engine::reliable_concepts();
    let labels: Vec<String> = concepts.iter()
        .take(8)
        .map(|c| c.label.clone())
        .collect();

    let mut count = 0;
    // Try triples
    for i in 0..labels.len() {
        for j in (i+1)..labels.len() {
            for k in (j+1)..labels.len() {
                let triple = vec![labels[i].clone(), labels[j].clone(), labels[k].clone()];
                if synthesize(triple).is_some() { count += 1; }
            }
        }
    }
    count
}

pub fn all_syntheses() -> Vec<SynthesizedConcept> {
    STATE.lock().map(|s| s.syntheses.clone()).unwrap_or_default()
}

pub fn recent_syntheses(n: usize) -> Vec<SynthesizedConcept> {
    STATE.lock()
        .map(|s| s.syntheses.iter().rev().take(n).cloned().collect())
        .unwrap_or_default()
}
