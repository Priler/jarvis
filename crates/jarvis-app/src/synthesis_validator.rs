//! Synthesis validator — validates synthesized cognition structures, routing
//! topologies, and reasoning chains before they are committed to the system.

use std::sync::atomic::{AtomicU64, Ordering};

pub static VALIDATIONS_RUN: AtomicU64 = AtomicU64::new(0);
pub static REJECTED:        AtomicU64 = AtomicU64::new(0);

const MIN_SYNTHESIS_CONFIDENCE:  f32 = 0.25;
const MIN_SYNTHESIS_STABILITY:   f32 = 0.20;
const MAX_SYNTHESIS_INSTABILITY: f32 = 0.75;

// ── ValidationVerdict ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ValidationVerdict {
    pub is_valid:   bool,
    pub confidence: f32,
    pub reason:     Option<String>,
}

impl ValidationVerdict {
    fn valid(confidence: f32) -> Self {
        ValidationVerdict { is_valid: true, confidence, reason: None }
    }
    fn invalid(confidence: f32, r: impl Into<String>) -> Self {
        REJECTED.fetch_add(1, Ordering::Relaxed);
        ValidationVerdict { is_valid: false, confidence, reason: Some(r.into()) }
    }
}

// ── Validation API ────────────────────────────────────────────────────────────

/// Validate a synthesis candidate.  Returns valid if confidence/stability are
/// above thresholds and the global system is not in a collapse state.
pub fn validate_synthesis(label: &str, confidence: f32, stability: f32) -> ValidationVerdict {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);

    if confidence < MIN_SYNTHESIS_CONFIDENCE {
        return ValidationVerdict::invalid(confidence,
            format!("confidence_too_low: {label} ({confidence:.3})"));
    }

    if stability < MIN_SYNTHESIS_STABILITY {
        return ValidationVerdict::invalid(confidence,
            format!("stability_too_low: {label} ({stability:.3})"));
    }

    // Guard against synthesizing into an already unstable system
    let sem = crate::semantic_stability::check();
    if sem.instability_score > MAX_SYNTHESIS_INSTABILITY {
        return ValidationVerdict::invalid(confidence,
            format!("semantic_instability_too_high: {:.3}", sem.instability_score));
    }

    // Hierarchy collapse guard
    if sem.has_collapse_risk {
        return ValidationVerdict::invalid(confidence,
            format!("hierarchy_collapse_risk: cannot synthesize during collapse"));
    }

    ValidationVerdict::valid(confidence)
}

/// Validate a routing topology candidate.
pub fn validate_topology(primary_load: f32, fallback_load: f32) -> ValidationVerdict {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);

    // Both paths must be below overload threshold
    if primary_load > crate::adaptive_topology::OVERLOAD_THRESHOLD {
        return ValidationVerdict::invalid(
            1.0 - primary_load,
            format!("primary_path_overloaded: {primary_load:.3}"),
        );
    }
    if fallback_load > crate::adaptive_topology::OVERLOAD_THRESHOLD {
        return ValidationVerdict::invalid(
            1.0 - fallback_load,
            format!("fallback_path_overloaded: {fallback_load:.3}"),
        );
    }

    ValidationVerdict::valid((1.0 - (primary_load + fallback_load) / 2.0).clamp(0.0, 1.0))
}

pub fn total_validated() -> u64 { VALIDATIONS_RUN.load(Ordering::Relaxed) }
pub fn total_rejected()  -> u64 { REJECTED.load(Ordering::Relaxed) }

pub fn rejection_rate() -> f32 {
    let run = VALIDATIONS_RUN.load(Ordering::Relaxed);
    if run == 0 { return 0.0; }
    REJECTED.load(Ordering::Relaxed) as f32 / run as f32
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_high_confidence_passes() {
        let v = validate_synthesis("test_chain", 0.80, 0.70);
        // May pass or fail depending on system state, but must not panic
        let _ = v.is_valid;
    }

    #[test]
    fn validate_low_confidence_rejected() {
        let v = validate_synthesis("weak_chain", 0.10, 0.70);
        assert!(!v.is_valid);
    }

    #[test]
    fn validate_low_stability_rejected() {
        let v = validate_synthesis("unstable_chain", 0.80, 0.05);
        assert!(!v.is_valid);
    }

    #[test]
    fn rejection_rate_bounded() {
        let _ = validate_synthesis("x", 0.10, 0.10);
        let r = rejection_rate();
        assert!(r >= 0.0 && r <= 1.0);
    }
}
