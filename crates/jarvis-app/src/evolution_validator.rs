//! Evolution validator — guards against hierarchy collapse, routing oscillation,
//! uncertainty runaway, and self-model corruption during topology evolution.

use std::sync::atomic::{AtomicU64, Ordering};

pub static VALIDATIONS_RUN:  AtomicU64 = AtomicU64::new(0);
pub static CHANGES_BLOCKED:  AtomicU64 = AtomicU64::new(0);

// How many topology changes in OSCILLATION_WINDOW events indicate oscillation
const OSCILLATION_WINDOW:          usize = 15;
const MAX_CHANGES_BEFORE_OSCILLATION: usize = 6;
const MAX_UNCERTAINTY_FOR_CHANGE:  f32   = 0.85;
const MIN_CONFIDENCE_FOR_CHANGE:   f32   = 0.15;

// ── ValidationResult ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    Approved,
    Rejected { reason: String },
}

impl ValidationResult {
    pub fn is_approved(&self) -> bool { matches!(self, Self::Approved) }
    pub fn reason(&self) -> Option<&str> {
        match self { Self::Rejected { reason } => Some(reason), _ => None }
    }
}

// ── Public validation API ─────────────────────────────────────────────────────

/// Validate a proposed topology change. Returns Approved if safe to apply.
pub fn validate_change(component: &str) -> ValidationResult {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);

    // 1. Hierarchy collapse
    if hierarchy_collapsing() {
        CHANGES_BLOCKED.fetch_add(1, Ordering::Relaxed);
        return ValidationResult::Rejected {
            reason: format!("hierarchy_collapse_risk: {component}"),
        };
    }

    // 2. Oscillation risk (too many recent topology changes)
    if oscillation_detected() {
        CHANGES_BLOCKED.fetch_add(1, Ordering::Relaxed);
        return ValidationResult::Rejected {
            reason: format!("oscillation_risk: {component} ({MAX_CHANGES_BEFORE_OSCILLATION} changes/{OSCILLATION_WINDOW} events)"),
        };
    }

    // 3. Uncertainty runaway
    if uncertainty_in_runaway() {
        CHANGES_BLOCKED.fetch_add(1, Ordering::Relaxed);
        return ValidationResult::Rejected {
            reason: format!("uncertainty_runaway: {component}"),
        };
    }

    // 4. Belief collapse (too few reliable beliefs to support routing)
    if belief_system_collapsed() {
        CHANGES_BLOCKED.fetch_add(1, Ordering::Relaxed);
        return ValidationResult::Rejected {
            reason: format!("belief_collapse: {component}"),
        };
    }

    ValidationResult::Approved
}

/// Check if a routing decision is safe.
pub fn validate_routing(confidence: f32) -> bool {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    confidence >= 0.20 && !hierarchy_collapsing()
}

// ── Internal checks ───────────────────────────────────────────────────────────

fn hierarchy_collapsing() -> bool {
    crate::semantic_stability::check().has_collapse_risk
}

fn oscillation_detected() -> bool {
    crate::topology_memory::recent_topology_changes(OSCILLATION_WINDOW)
        >= MAX_CHANGES_BEFORE_OSCILLATION
}

fn uncertainty_in_runaway() -> bool {
    crate::uncertainty_graph::avg_uncertainty() > MAX_UNCERTAINTY_FOR_CHANGE
}

fn belief_system_collapsed() -> bool {
    let avg = crate::belief_engine::avg_confidence();
    let count = crate::belief_engine::belief_count();
    count > 5 && avg < MIN_CONFIDENCE_FOR_CHANGE
}

// ── Summary ───────────────────────────────────────────────────────────────────

pub struct ValidatorSummary {
    pub validations_run:  u64,
    pub changes_blocked:  u64,
    pub block_rate:       f32,
}

pub fn summary() -> ValidatorSummary {
    let run = VALIDATIONS_RUN.load(Ordering::Relaxed);
    let blocked = CHANGES_BLOCKED.load(Ordering::Relaxed);
    ValidatorSummary {
        validations_run: run,
        changes_blocked: blocked,
        block_rate: if run == 0 { 0.0 } else { blocked as f32 / run as f32 },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_change_no_panic() {
        let r = validate_change("test_component");
        let _ = r.is_approved();
    }

    #[test]
    fn validate_routing_reasonable_confidence() {
        // With no collapse risk and default state, high confidence should pass
        assert!(validate_routing(0.80));
        assert!(!validate_routing(0.10));
    }
}
