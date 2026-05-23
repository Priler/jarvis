//! Symbolic safety system — guards against hallucinated inference chains,
//! false symbolic conclusions, recursive semantic instability, and
//! contradiction cascades.

use std::sync::atomic::{AtomicU64, Ordering};

pub static INFERENCES_REJECTED:  AtomicU64 = AtomicU64::new(0);
pub static SYNTHESES_REJECTED:   AtomicU64 = AtomicU64::new(0);
pub static VALIDATIONS_RUN:      AtomicU64 = AtomicU64::new(0);
pub static CASCADE_ALERTS:       AtomicU64 = AtomicU64::new(0);

// ── Thresholds ────────────────────────────────────────────────────────────────

pub const MAX_INFERENCE_DEPTH:    usize = 8;
pub const MIN_CHAIN_CONFIDENCE:   f32   = 0.25;
pub const MAX_CONTRADICTIONS_PER_CYCLE: usize = 10;
pub const MIN_SYNTHESIS_CONFIDENCE: f32 = 0.30;

// ── Validation result ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SymbolicVerdict {
    Valid,
    Rejected { reason: String },
}

impl SymbolicVerdict {
    pub fn is_valid(&self) -> bool { matches!(self, SymbolicVerdict::Valid) }
}

// ── Chain validation ──────────────────────────────────────────────────────────

pub fn validate_chain(depth: usize, confidence: f32) -> SymbolicVerdict {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);

    if depth > MAX_INFERENCE_DEPTH {
        INFERENCES_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SymbolicVerdict::Rejected {
            reason: format!("depth_exceeded:{}", depth),
        };
    }
    if confidence < MIN_CHAIN_CONFIDENCE {
        INFERENCES_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SymbolicVerdict::Rejected {
            reason: format!("confidence_too_low:{:.2}", confidence),
        };
    }
    SymbolicVerdict::Valid
}

// ── Synthesis validation ──────────────────────────────────────────────────────

pub fn validate_synthesis(source_count: usize, confidence: f32) -> SymbolicVerdict {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);

    if source_count < 2 {
        SYNTHESES_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SymbolicVerdict::Rejected {
            reason: "insufficient_sources".to_string(),
        };
    }
    if confidence < MIN_SYNTHESIS_CONFIDENCE {
        SYNTHESES_REJECTED.fetch_add(1, Ordering::Relaxed);
        return SymbolicVerdict::Rejected {
            reason: format!("synthesis_confidence_too_low:{:.2}", confidence),
        };
    }
    SymbolicVerdict::Valid
}

// ── Contradiction cascade guard ───────────────────────────────────────────────

/// Returns true if a cascade is detected (too many contradictions in one cycle).
pub fn check_contradiction_cascade(contradiction_count: usize) -> bool {
    VALIDATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    if contradiction_count >= MAX_CONTRADICTIONS_PER_CYCLE {
        CASCADE_ALERTS.fetch_add(1, Ordering::Relaxed);
        return true;
    }
    false
}

// ── Symbolic loop guard ───────────────────────────────────────────────────────

/// Checks whether two entity labels form a suspected circular inference.
pub fn is_circular(source: &str, target: &str) -> bool {
    source == target
}

// ── Summary ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolicSafetySummary {
    pub validations_run:      u64,
    pub inferences_rejected:  u64,
    pub syntheses_rejected:   u64,
    pub cascade_alerts:       u64,
}

pub fn summary() -> SymbolicSafetySummary {
    SymbolicSafetySummary {
        validations_run:     VALIDATIONS_RUN.load(Ordering::Relaxed),
        inferences_rejected: INFERENCES_REJECTED.load(Ordering::Relaxed),
        syntheses_rejected:  SYNTHESES_REJECTED.load(Ordering::Relaxed),
        cascade_alerts:      CASCADE_ALERTS.load(Ordering::Relaxed),
    }
}
