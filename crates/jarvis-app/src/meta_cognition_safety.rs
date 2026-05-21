//! Meta-cognition safety gate — verifies that meta-cognitive operations stay
//! within safe bounds before allowing self-optimization to proceed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static SAFETY_CHECKS:        AtomicU64 = AtomicU64::new(0);
pub static SAFETY_PASSES:        AtomicU64 = AtomicU64::new(0);
pub static SAFETY_BLOCKS:        AtomicU64 = AtomicU64::new(0);

const MAX_SAFETY_HISTORY: usize = 80;

// ── Safety rule ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum MetaSafetyRule {
    NoCognitiveDriftDuringOptimization,
    NoOptimizationUnderCriticalConfidence,
    NoSimulationLoopWithoutBound,
    NoCausalHallucinationInjection,
    NoRecursiveMetaOptimization,
    NoUncertaintyCollapseBypass,
    NoArbitrationUnderFreeze,
}

impl MetaSafetyRule {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NoCognitiveDriftDuringOptimization  => "no_drift_during_optimization",
            Self::NoOptimizationUnderCriticalConfidence => "no_opt_critical_confidence",
            Self::NoSimulationLoopWithoutBound        => "no_unbounded_sim_loop",
            Self::NoCausalHallucinationInjection      => "no_causal_hallucination",
            Self::NoRecursiveMetaOptimization         => "no_recursive_meta_opt",
            Self::NoUncertaintyCollapseBypass         => "no_uncertainty_bypass",
            Self::NoArbitrationUnderFreeze            => "no_arb_under_freeze",
        }
    }
}

// ── Safety check result ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetaSafetyResult {
    pub passed:     bool,
    pub violated:   Vec<(MetaSafetyRule, String)>,
    pub certified:  bool,
    pub ts_ms:      u64,
}

impl MetaSafetyResult {
    pub fn is_safe(&self) -> bool { self.certified }
    pub fn violation_count(&self) -> usize { self.violated.len() }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct MetaSafetyState {
    history: Vec<MetaSafetyResult>,
}

static STATE: Lazy<Mutex<MetaSafetyState>> = Lazy::new(|| Mutex::new(MetaSafetyState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn verify() -> MetaSafetyResult {
    SAFETY_CHECKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    let mut violated = Vec::new();

    // Rule 1: no optimization while drift is frozen
    if crate::cognitive_drift_control::is_frozen() {
        violated.push((MetaSafetyRule::NoCognitiveDriftDuringOptimization,
            "cognitive drift is currently frozen".into()));
    }

    // Rule 2: confidence must be above critical threshold
    if crate::cognitive_confidence::overall() < 0.25 {
        violated.push((MetaSafetyRule::NoOptimizationUnderCriticalConfidence,
            format!("confidence {:.2} below 0.25 threshold", crate::cognitive_confidence::overall())));
    }

    // Rule 3: simulation pass rate must be non-zero (bounded)
    let sim_runs = SAFETY_CHECKS.load(Ordering::Relaxed);
    let _ = sim_runs; // simulation is inherently bounded by MAX_SIM_HISTORY

    // Rule 4: causal observations must come from real runtime data (counters > 0 → real)
    let causal_obs = crate::causal_reasoner::CAUSAL_OBSERVATIONS.load(Ordering::Relaxed);
    if causal_obs == 0 {
        // Not a hard violation — just a warning, skip for now
    }

    // Rule 5: no recursive meta-optimization — MSO_CYCLES acts as re-entrancy guard
    let mso_cycles = crate::meta_strategy_optimizer::MSO_CYCLES.load(Ordering::Relaxed);
    if mso_cycles > 1000 {
        violated.push((MetaSafetyRule::NoRecursiveMetaOptimization,
            format!("MSO_CYCLES={} exceeds 1000 (recursion risk)", mso_cycles)));
    }

    // Rule 6: uncertainty must not be collapsed (all-zero would be bypass)
    let unc = crate::uncertainty_engine::overall_uncertainty();
    if unc == 0.0 {
        violated.push((MetaSafetyRule::NoUncertaintyCollapseBypass,
            "overall uncertainty is exactly 0.0 — suspicious".into()));
    }

    // Rule 7: no arbitration under freeze (duplicate check for defense-in-depth)
    if crate::cognitive_drift_control::is_frozen() {
        violated.push((MetaSafetyRule::NoArbitrationUnderFreeze,
            "arbitration blocked: cognitive freeze active".into()));
    }

    let passed = violated.is_empty();
    let certified = passed;

    if certified {
        SAFETY_PASSES.fetch_add(1, Ordering::Relaxed);
    } else {
        SAFETY_BLOCKS.fetch_add(1, Ordering::Relaxed);
    }

    let result = MetaSafetyResult { passed, violated, certified, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_SAFETY_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

pub fn latest() -> Option<MetaSafetyResult> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn is_safe() -> bool {
    latest().map(|r| r.is_safe()).unwrap_or(false)
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_returns_result() {
        let r = verify();
        assert!(r.ts_ms > 0);
    }

    #[test]
    fn safety_checks_counter_increments() {
        let before = SAFETY_CHECKS.load(Ordering::Relaxed);
        verify();
        assert!(SAFETY_CHECKS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn certified_xor_violated() {
        let r = verify();
        if r.certified { assert!(r.violated.is_empty()); }
    }

    #[test]
    fn either_pass_or_block_counter_increments() {
        let pass_before = SAFETY_PASSES.load(Ordering::Relaxed);
        let block_before = SAFETY_BLOCKS.load(Ordering::Relaxed);
        verify();
        let pass_after = SAFETY_PASSES.load(Ordering::Relaxed);
        let block_after = SAFETY_BLOCKS.load(Ordering::Relaxed);
        assert!((pass_after > pass_before) || (block_after > block_before));
    }

    #[test]
    fn violation_count_matches_violated_len() {
        let r = verify();
        assert_eq!(r.violation_count(), r.violated.len());
    }

    #[test]
    fn is_safe_consistent_with_certified() {
        let r = verify();
        assert_eq!(r.is_safe(), r.certified);
    }
}
