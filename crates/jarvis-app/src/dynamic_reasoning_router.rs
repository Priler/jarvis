//! Dynamic reasoning router — selects the best cognition path for the current
//! tick based on uncertainty level, resource pressure, stability, and load.
//! Routing decisions are validated and logged to topology_memory.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static ROUTING_DECISIONS: AtomicU64 = AtomicU64::new(0);
pub static ROUTES_OVERRIDDEN:  AtomicU64 = AtomicU64::new(0);

use crate::adaptive_topology::CognitionPath;

const MAX_HISTORY: usize = 200;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── RoutingDecision ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub chosen_path: CognitionPath,
    pub confidence:  f32,
    pub reason:      String,
    pub ts_ms:       u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<RoutingDecision>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Routing logic ─────────────────────────────────────────────────────────────

/// Route the current reasoning tick to the optimal cognition path.
pub fn route() -> RoutingDecision {
    let unc       = crate::generalized_uncertainty::profile();
    let resource  = crate::abstract_resource_reasoner::sample();
    let sem_stab  = crate::semantic_stability::check();
    let prob_stab = crate::probabilistic_stability::check();
    let cog_stab  = crate::cognitive_stability::check();

    // Primary routing heuristics (ordered by priority)
    let (path, reason, confidence) = if sem_stab.has_collapse_risk {
        // Semantic collapse: route to hierarchical to restructure
        (CognitionPath::Hierarchical, "semantic_collapse_risk", 0.85_f32)

    } else if prob_stab.has_belief_collapse || prob_stab.has_uncertainty_explosion {
        // Probabilistic instability: fall back to meta-cognition for oversight
        (CognitionPath::MetaCognition, "probabilistic_instability", 0.80_f32)

    } else if resource.is_overloaded() {
        // Resource overload: use lightweight path to reduce cost
        (CognitionPath::Lightweight, "resource_overload", 0.90_f32)

    } else if unc.overall > 0.70 {
        // High uncertainty: route to probabilistic which handles it best
        (CognitionPath::Probabilistic, "high_uncertainty", (1.0 - unc.overall + 0.30).clamp(0.3, 0.9))

    } else if !cog_stab.is_stable {
        // Cognitive oscillation: route to meta-cognition
        (CognitionPath::MetaCognition, "cognitive_oscillation", 0.65_f32)

    } else if unc.semantic_uncertainty > 0.55 {
        // Semantic uncertainty: route to conceptual reasoning
        (CognitionPath::Conceptual, "semantic_uncertainty", (1.0 - unc.semantic_uncertainty + 0.45).clamp(0.35, 0.85))

    } else if unc.overall < 0.30 && !resource.is_overloaded() {
        // Low uncertainty, good resources: optimal symbolic reasoning
        (CognitionPath::Symbolic, "low_uncertainty_optimal", 0.90_f32)

    } else {
        // Default: use adaptive topology recommendation
        let recommended = crate::adaptive_topology::recommended_path();
        let weight      = crate::adaptive_topology::get_weight(recommended);
        (recommended, "topology_recommended", (0.50 + weight * 0.40).clamp(0.40, 0.85))
    };

    // Check if chosen path is suppressed — fall back to recommended if so
    let suppressed = crate::adaptive_topology::suppressed_paths();
    let (final_path, final_confidence, final_reason) = if suppressed.contains(&path) {
        let fallback = crate::adaptive_topology::recommended_path();
        ROUTES_OVERRIDDEN.fetch_add(1, Ordering::Relaxed);
        (fallback, confidence * 0.80, format!("{reason}_fallback_from_suppressed"))
    } else {
        (path, confidence, reason.to_string())
    };

    // Validate the routing decision
    let valid = crate::evolution_validator::validate_routing(final_confidence);
    let (validated_path, validated_conf) = if valid {
        (final_path, final_confidence)
    } else {
        ROUTES_OVERRIDDEN.fetch_add(1, Ordering::Relaxed);
        (CognitionPath::Lightweight, 0.40_f32)
    };

    let decision = RoutingDecision {
        chosen_path: validated_path,
        confidence:  validated_conf,
        reason:      final_reason.clone(),
        ts_ms:       ts_now(),
    };

    // Log to topology_memory
    crate::topology_memory::record(crate::topology_memory::TopologyEvent::RoutingDecision {
        from:   "router".into(),
        to:     validated_path.name().into(),
        reason: final_reason,
    });

    // Store in history
    let mut h = HISTORY.lock().unwrap();
    if h.len() >= MAX_HISTORY { h.remove(0); }
    h.push(decision.clone());

    ROUTING_DECISIONS.fetch_add(1, Ordering::Relaxed);
    decision
}

pub fn latest() -> Option<RoutingDecision> {
    HISTORY.lock().unwrap().last().cloned()
}

pub fn recent(n: usize) -> Vec<RoutingDecision> {
    HISTORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

/// Frequency distribution of routing decisions.
pub fn path_frequencies() -> Vec<(CognitionPath, usize)> {
    let h = HISTORY.lock().unwrap();
    let mut counts = std::collections::HashMap::new();
    for d in h.iter() {
        *counts.entry(d.chosen_path).or_insert(0usize) += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_returns_valid_decision() {
        let d = route();
        assert!(d.confidence >= 0.0 && d.confidence <= 1.0);
        assert!(!d.reason.is_empty());
    }

    #[test]
    fn path_frequencies_no_panic() {
        route();
        let _ = path_frequencies();
    }
}
