//! Probabilistic safety engine — guards against uncertainty explosion,
//! confidence hallucination, belief collapse, and self-model corruption.

use std::sync::atomic::{AtomicU64, Ordering};

pub static VALIDATIONS_RUN:    AtomicU64 = AtomicU64::new(0);
pub static BELIEFS_REJECTED:   AtomicU64 = AtomicU64::new(0);
pub static SAFETY_GATES_FIRED: AtomicU64 = AtomicU64::new(0);

pub const MIN_BELIEF_CONFIDENCE:      f32   = 0.10;
pub const MAX_CONTRADICTION_PRESSURE: f32   = 0.85;
pub const MAX_UNCERTAINTY_THRESHOLD:  f32   = 0.85;
pub const MAX_PROPAGATION_DEPTH:      usize = 6;

// ── SafetyVerdict ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyVerdict {
    Approved,
    RejectedLowConfidence,
    RejectedHighPressure,
    RejectedUnstable,
}

// ── Validation ────────────────────────────────────────────────────────────────

pub fn validate_belief(confidence: f32, contradiction_pressure: f32, stability: f32) -> SafetyVerdict {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    if confidence < MIN_BELIEF_CONFIDENCE {
        BELIEFS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SafetyVerdict::RejectedLowConfidence;
    }
    if contradiction_pressure > MAX_CONTRADICTION_PRESSURE {
        BELIEFS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SafetyVerdict::RejectedHighPressure;
    }
    if stability < 0.10 {
        BELIEFS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SafetyVerdict::RejectedUnstable;
    }
    SafetyVerdict::Approved
}

pub fn check_uncertainty_explosion() -> bool {
    let avg_u = crate::uncertainty_graph::avg_uncertainty();
    if avg_u > MAX_UNCERTAINTY_THRESHOLD {
        SAFETY_GATES_FIRED.fetch_add(1, Ordering::Relaxed);
        crate::probabilistic_observability::log(
            crate::probabilistic_observability::ProbabilisticEvent::SafetyGateFired {
                reason: format!("uncertainty_explosion: avg_uncertainty={avg_u:.3}"),
            }
        );
        return true;
    }
    false
}

pub fn check_belief_collapse() -> bool {
    let avg_conf = crate::belief_engine::avg_confidence();
    if avg_conf < 0.15 && crate::belief_engine::belief_count() > 5 {
        SAFETY_GATES_FIRED.fetch_add(1, Ordering::Relaxed);
        crate::probabilistic_observability::log(
            crate::probabilistic_observability::ProbabilisticEvent::SafetyGateFired {
                reason: format!("belief_collapse: avg_confidence={avg_conf:.3}"),
            }
        );
        return true;
    }
    false
}

pub fn check_propagation_depth(depth: usize) -> bool {
    depth <= MAX_PROPAGATION_DEPTH
}

// ── Summary ───────────────────────────────────────────────────────────────────

pub struct SafetySummary {
    pub validations_run:    u64,
    pub beliefs_rejected:   u64,
    pub safety_gates_fired: u64,
}

pub fn summary() -> SafetySummary {
    SafetySummary {
        validations_run:    VALIDATIONS_RUN.load(Ordering::Relaxed),
        beliefs_rejected:   BELIEFS_REJECTED.load(Ordering::Relaxed),
        safety_gates_fired: SAFETY_GATES_FIRED.load(Ordering::Relaxed),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_low_confidence() {
        let v = validate_belief(0.05, 0.0, 0.5);
        assert_eq!(v, SafetyVerdict::RejectedLowConfidence);
    }

    #[test]
    fn rejects_high_pressure() {
        let v = validate_belief(0.7, 0.9, 0.5);
        assert_eq!(v, SafetyVerdict::RejectedHighPressure);
    }

    #[test]
    fn approves_valid_belief() {
        let v = validate_belief(0.7, 0.2, 0.5);
        assert_eq!(v, SafetyVerdict::Approved);
    }

    #[test]
    fn propagation_depth_gate() {
        assert!(check_propagation_depth(4));
        assert!(!check_propagation_depth(7));
    }
}
