//! Recursive orchestrator — coordinates recursive cognition loops, supervises
//! synthesis recursion, and suppresses unstable recursive growth.
//! Hard depth limit: MAX_SAFE_RECURSION_DEPTH (8).

use std::sync::atomic::{AtomicU64, Ordering};

pub static CYCLES_ORCHESTRATED: AtomicU64 = AtomicU64::new(0);
pub static CYCLES_SUPPRESSED:   AtomicU64 = AtomicU64::new(0);

// ── OrchestrationResult ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OrchestrationResult {
    pub depth:          u32,
    pub cycles_run:     usize,
    pub safe:           bool,
    pub stability_score: f32,
}

// ── Orchestration logic ───────────────────────────────────────────────────────

/// Run one orchestration cycle.  Enters recursion, executes sub-cycles, exits.
pub fn orchestrate() -> OrchestrationResult {
    let safe = crate::distributed_safety::check_recursive_safe();
    if !safe.is_safe {
        CYCLES_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        crate::ai_os_observability::record(
            crate::ai_os_observability::AiOsEvent::SafetyGate {
                component: "recursive_orchestrator".into(),
                reason: safe.reason.unwrap_or_else(|| "recursive_limit".into()),
            }
        );
        return OrchestrationResult { depth: 0, cycles_run: 0, safe: false, stability_score: 0.0 };
    }

    let entered = crate::recursive_stability::enter_recursion();
    if !entered {
        CYCLES_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return OrchestrationResult { depth: 0, cycles_run: 0, safe: false, stability_score: 0.0 };
    }

    let depth = crate::recursive_stability::current_depth();
    let mut cycles_run = 0;

    // Sub-cycle 1: Coordinate adaptive reasoning
    let _ = crate::dynamic_reasoning_router::route();
    cycles_run += 1;

    // Sub-cycle 2: Supervise synthesis recursion (one level only — no nested orchestrate())
    let _ = crate::self_optimization::optimize();
    cycles_run += 1;

    // Sub-cycle 3: Check recursive stability
    let stab = crate::recursive_stability::check();
    cycles_run += 1;

    crate::recursive_stability::exit_recursion();
    CYCLES_ORCHESTRATED.fetch_add(1, Ordering::Relaxed);

    crate::ai_os_observability::record(
        crate::ai_os_observability::AiOsEvent::RecursionDepth {
            depth,
            safe: stab.overall_stable,
        }
    );

    OrchestrationResult {
        depth,
        cycles_run,
        safe: stab.overall_stable,
        stability_score: 1.0 - stab.risk_score,
    }
}

pub fn cycles_orchestrated() -> u64 { CYCLES_ORCHESTRATED.load(Ordering::Relaxed) }
pub fn cycles_suppressed()   -> u64 { CYCLES_SUPPRESSED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orchestrate_no_panic() {
        let r = orchestrate();
        let _ = r.safe;
    }

    #[test]
    fn depth_bounded_after_orchestrate() {
        for _ in 0..5 { let _ = orchestrate(); }
        let depth = crate::recursive_stability::current_depth();
        assert!(depth <= crate::recursive_stability::MAX_SAFE_RECURSION_DEPTH);
    }

    #[test]
    fn stability_score_bounded() {
        let r = orchestrate();
        assert!(r.stability_score >= 0.0 && r.stability_score <= 1.0);
    }
}
