//! Conceptual safety system — validates abstractions and transfers before
//! propagation. Prevents hallucinated abstractions, false analogies,
//! over-generalized patterns, and unrelated domain collapse.

use std::sync::atomic::{AtomicU64, Ordering};

pub static CONCEPTS_REJECTED:  AtomicU64 = AtomicU64::new(0);
pub static TRANSFERS_REJECTED: AtomicU64 = AtomicU64::new(0);
pub static VALIDATIONS_RUN:    AtomicU64 = AtomicU64::new(0);

// ── Thresholds ────────────────────────────────────────────────────────────────

const MIN_CONCEPT_CONFIDENCE:  f32   = 0.35;
const MAX_ABSTRACTION_LEVEL:   u8    = 3;
const MIN_TRANSFER_SIMILARITY: f32   = 0.50;
const MAX_TRANSFER_DEPTH:      usize = 4;   // max chained transfer steps

// ── SafetyVerdict ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SafetyVerdict {
    Valid,
    Rejected { reason: String },
}

impl SafetyVerdict {
    pub fn is_valid(&self) -> bool { matches!(self, SafetyVerdict::Valid) }
}

// ── Concept validation ────────────────────────────────────────────────────────

/// Validate a concept before it is propagated or transferred.
pub fn validate_concept(concept: &crate::concept_engine::Concept) -> SafetyVerdict {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);

    if concept.confidence < MIN_CONCEPT_CONFIDENCE {
        CONCEPTS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SafetyVerdict::Rejected {
            reason: format!("confidence_too_low:{:.2}", concept.confidence),
        };
    }
    if concept.abstraction_level > MAX_ABSTRACTION_LEVEL {
        CONCEPTS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SafetyVerdict::Rejected {
            reason: format!("abstraction_level_too_high:{}", concept.abstraction_level),
        };
    }
    if concept.observation_count < 3 {
        CONCEPTS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SafetyVerdict::Rejected {
            reason: format!("insufficient_observations:{}", concept.observation_count),
        };
    }
    if concept.label.is_empty() {
        CONCEPTS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SafetyVerdict::Rejected { reason: "empty_label".to_string() };
    }
    SafetyVerdict::Valid
}

// ── Transfer validation ───────────────────────────────────────────────────────

/// Validate an analogical transfer from one concept label to another.
/// Returns true if the transfer is safe to apply.
pub fn validate_transfer(source: &str, target: &str, similarity: f32) -> bool {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);

    // Reject trivially unrelated domains
    if similarity < MIN_TRANSFER_SIMILARITY {
        TRANSFERS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return false;
    }

    // Reject circular self-transfer
    if source == target {
        TRANSFERS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return false;
    }

    // Reject if source concept has insufficient evidence
    if let Some(src_concept) = crate::concept_engine::best_match(source) {
        if src_concept.confidence < MIN_CONCEPT_CONFIDENCE {
            TRANSFERS_REJECTED.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if src_concept.observation_count < 3 {
            TRANSFERS_REJECTED.fetch_add(1, Ordering::Relaxed);
            return false;
        }
    }

    // Check for domain incompatibility (hardware ↔ planning is not a valid analogy)
    let incompatible_pairs = [
        ("hardware", "planning"),
        ("audio",    "strategy"),
        ("pixel",    "cognition"),
    ];
    let s_lower = source.to_lowercase();
    let t_lower = target.to_lowercase();
    for (a, b) in &incompatible_pairs {
        if (s_lower.contains(a) && t_lower.contains(b))
        || (s_lower.contains(b) && t_lower.contains(a)) {
            TRANSFERS_REJECTED.fetch_add(1, Ordering::Relaxed);
            return false;
        }
    }

    true
}

// ── Transfer chain depth guard ────────────────────────────────────────────────

/// Check that a proposed transfer chain does not exceed MAX_TRANSFER_DEPTH.
pub fn check_transfer_depth(depth: usize) -> bool {
    if depth > MAX_TRANSFER_DEPTH {
        TRANSFERS_REJECTED.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    true
}

// ── Summary ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SafetySummary {
    pub validations_run:   u64,
    pub concepts_rejected: u64,
    pub transfers_rejected: u64,
    pub rejection_rate:    f32,
}

pub fn summary() -> SafetySummary {
    let run  = VALIDATIONS_RUN.load(Ordering::Relaxed);
    let crej = CONCEPTS_REJECTED.load(Ordering::Relaxed);
    let trej = TRANSFERS_REJECTED.load(Ordering::Relaxed);
    let total_rejected = crej + trej;
    SafetySummary {
        validations_run:   run,
        concepts_rejected: crej,
        transfers_rejected: trej,
        rejection_rate: if run == 0 { 0.0 } else { total_rejected as f32 / run as f32 },
    }
}
