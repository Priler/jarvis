//! Self-improvement runtime — top-level orchestrator for the Phase 16 pipeline.
//! Drives: evaluate → drift-check → evolve → adapt → verify safety → report.
//! All improvements are bounded, heuristic-only, local, and gated by safe_adaptation.

use std::sync::atomic::{AtomicU64, Ordering};

pub static IMPROVEMENT_CYCLES:  AtomicU64 = AtomicU64::new(0);
pub static QUALITY_IMPROVEMENTS: AtomicU64 = AtomicU64::new(0);
pub static CYCLES_BLOCKED:      AtomicU64 = AtomicU64::new(0);

// ── Improvement cycle result ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImprovementCycle {
    pub cycle_id:           u64,
    pub generation:         u32,
    pub quality_before:     f32,
    pub quality_after:      f32,
    pub quality_delta:      f32,
    pub drift_frozen:       bool,
    pub adaptation_applied: bool,
    pub safety_certified:   bool,
    pub ts_ms:              u64,
}

impl ImprovementCycle {
    pub fn improved(&self) -> bool { self.quality_delta > 0.01 }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Run one complete self-improvement cycle.
pub fn run_cycle() -> ImprovementCycle {
    let cycle_id = IMPROVEMENT_CYCLES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    // Quality before
    let quality_before = crate::execution_quality::average_overall(5);

    // Step 1: Drift check — if frozen, skip improvement
    let drift_frozen = crate::cognitive_drift_control::is_frozen();
    if drift_frozen {
        CYCLES_BLOCKED.fetch_add(1, Ordering::Relaxed);
        let quality_after = crate::execution_quality::measure().overall;
        return ImprovementCycle {
            cycle_id, generation: crate::cognitive_evolution::generation(),
            quality_before, quality_after, quality_delta: quality_after - quality_before,
            drift_frozen: true, adaptation_applied: false, safety_certified: false,
            ts_ms: now,
        };
    }

    // Step 2: Run feedback loop (evaluate → reflect → optimize → adapt)
    let fb = crate::feedback_loop::run_tick();

    // Step 3: Safety verification
    let safety_result = crate::autonomous_learning_safety::verify();
    let safety_certified = safety_result.is_certified();

    // Step 4: Self-evaluation report
    crate::self_evaluation::evaluate();

    // Step 5: Strategic observability snapshot
    let snap = crate::strategic_observability::snapshot();

    // Quality after
    let quality_after = snap.current_overall_quality;
    let quality_delta = quality_after - quality_before;

    if quality_delta > 0.01 {
        QUALITY_IMPROVEMENTS.fetch_add(1, Ordering::Relaxed);
    }

    ImprovementCycle {
        cycle_id,
        generation:         crate::cognitive_evolution::generation(),
        quality_before,
        quality_after,
        quality_delta,
        drift_frozen:       false,
        adaptation_applied: fb.adaptation_applied,
        safety_certified,
        ts_ms:              now,
    }
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
    fn run_cycle_completes_without_panic() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let _c = run_cycle();
    }

    #[test]
    fn improvement_cycles_increments() {
        let before = IMPROVEMENT_CYCLES.load(Ordering::Relaxed);
        crate::cognitive_drift_control::unfreeze_for_test();
        run_cycle();
        assert!(IMPROVEMENT_CYCLES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn quality_delta_is_finite() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let c = run_cycle();
        assert!(c.quality_delta.is_finite());
    }

    #[test]
    fn quality_scores_bounded() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let c = run_cycle();
        assert!(c.quality_before >= 0.0 && c.quality_before <= 1.0);
        assert!(c.quality_after  >= 0.0 && c.quality_after  <= 1.0);
    }

    #[test]
    fn frozen_drift_blocks_cycle() {
        crate::cognitive_drift_control::freeze_for_test();
        let c = run_cycle();
        assert!(c.drift_frozen);
        assert!(!c.adaptation_applied);
        crate::cognitive_drift_control::unfreeze_for_test();
    }

    #[test]
    fn multiple_cycles_stable() {
        crate::cognitive_drift_control::unfreeze_for_test();
        for _ in 0..5 { run_cycle(); }
        assert!(IMPROVEMENT_CYCLES.load(Ordering::Relaxed) >= 5);
    }
}
