//! Generalized reasoning engine — top-level reasoning over concepts, patterns,
//! abstractions, conceptual hierarchies, and generalized operational structures.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static GENERALIZED_CYCLES: AtomicU64 = AtomicU64::new(0);

const MAX_RESULTS: usize = 100;

// ── GeneralizedInsight ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GeneralizedInsight {
    ConceptualPatternFound { pattern: String, strength: f32 },
    CrossDomainOpportunity { from: String, to: String },
    AbstractGoalAlignment { goal_id: u64, concept: String },
    SystemicRisk { dimension: String, level: f32 },
    OptimizationOpportunity { motif: String, potential: f32 },
}

// ── GeneralizedReasoningResult ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeneralizedReasoningResult {
    pub cycle_id:    u64,
    pub insights:    Vec<GeneralizedInsight>,
    pub abstraction_quality: f32,
    pub transfer_success_rate: f32,
    pub world_state: String,
    pub recommendation: String,
    pub healthy:     bool,
    pub ts_ms:       u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct GeneralizedState {
    results: Vec<GeneralizedReasoningResult>,
}

static STATE: Lazy<Mutex<GeneralizedState>> = Lazy::new(|| Mutex::new(GeneralizedState {
    results: Vec::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Reasoning ────────────────────────────────────────────────────────────────

pub fn reason() -> GeneralizedReasoningResult {
    let cycle_id = GENERALIZED_CYCLES.fetch_add(1, Ordering::Relaxed) + 1;
    let now = ts_now();
    let mut insights = Vec::new();

    // 1. Strong semantic structures → pattern insights
    let structures = crate::semantic_structures::strong_structures();
    for st in structures.iter().take(5) {
        insights.push(GeneralizedInsight::ConceptualPatternFound {
            pattern:  st.label.clone(),
            strength: st.confidence,
        });
    }

    // 2. Valid analogical transfers → cross-domain opportunities
    let analogies = crate::analogical_reasoner::recent_mappings(10);
    for mapping in &analogies {
        if mapping.validated {
            insights.push(GeneralizedInsight::CrossDomainOpportunity {
                from: mapping.source_label.clone(),
                to:   mapping.target_label.clone(),
            });
        }
    }

    // 3. Abstract goals aligned with concepts
    let active_goals = crate::abstract_goals::active();
    for goal in &active_goals {
        if let Some(ref concept_label) = goal.linked_concept {
            insights.push(GeneralizedInsight::AbstractGoalAlignment {
                goal_id: goal.id,
                concept: concept_label.clone(),
            });
        }
    }

    // 4. World model fragility → systemic risk
    if let Some(world) = crate::conceptual_world_model::latest() {
        if world.strategic_fragility > 0.6 {
            insights.push(GeneralizedInsight::SystemicRisk {
                dimension: "strategic_fragility".to_string(),
                level:     world.strategic_fragility,
            });
        }
        if world.dependency_risk > 0.65 {
            insights.push(GeneralizedInsight::SystemicRisk {
                dimension: "dependency_risk".to_string(),
                level:     world.dependency_risk,
            });
        }
    }

    // 5. Optimization motifs from semantic structures
    let opt_structures = crate::semantic_structures::by_kind(
        &crate::semantic_structures::StructureKind::RecurringOptimization);
    for opt in opt_structures.iter().take(3) {
        insights.push(GeneralizedInsight::OptimizationOpportunity {
            motif:     opt.label.clone(),
            potential: opt.confidence,
        });
    }

    // Compute quality metrics
    let concept_reasoning = crate::conceptual_reasoner::latest();
    let abstraction_quality = concept_reasoning.as_ref().map(|r| r.quality).unwrap_or(0.5);

    let transfers = crate::transfer_reasoning::all_validated().len();
    let attempted = crate::transfer_reasoning::TRANSFERS_ATTEMPTED.load(Ordering::Relaxed);
    let transfer_success_rate = if attempted == 0 { 0.0 }
        else { transfers as f32 / attempted as f32 };

    // World state description
    let world_state = crate::conceptual_world_model::latest()
        .map(|w| format!("{:?}", w.abstract_state))
        .unwrap_or_else(|| "unknown".to_string());

    // Recommendation
    let recommendation = if abstraction_quality < 0.4 {
        "enrich_concept_observations".to_string()
    } else if transfer_success_rate < 0.3 && attempted > 5 {
        "improve_analogy_confidence".to_string()
    } else if insights.iter().any(|i| matches!(i, GeneralizedInsight::SystemicRisk { level, .. } if *level > 0.75)) {
        "address_systemic_risk".to_string()
    } else {
        "continue_generalized_reasoning".to_string()
    };

    let healthy = abstraction_quality >= 0.3 && transfer_success_rate >= 0.0;

    let result = GeneralizedReasoningResult {
        cycle_id, insights, abstraction_quality, transfer_success_rate,
        world_state, recommendation, healthy, ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.results.len() >= MAX_RESULTS { s.results.remove(0); }
        s.results.push(result.clone());
    }
    result
}

pub fn latest() -> Option<GeneralizedReasoningResult> {
    STATE.lock().ok().and_then(|s| s.results.last().cloned())
}

pub fn history() -> Vec<GeneralizedReasoningResult> {
    STATE.lock().map(|s| s.results.clone()).unwrap_or_default()
}
