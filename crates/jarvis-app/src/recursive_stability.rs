//! Recursive stability engine — detects recursive amplification, distributed
//! overload, scheduler collapse, service starvation, and simulation runaway.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub static MAX_SAFE_RECURSION_DEPTH: u32 = 8;

pub static CURRENT_RECURSION_DEPTH:  AtomicU32 = AtomicU32::new(0);
pub static AMPLIFICATION_EVENTS:     AtomicU64 = AtomicU64::new(0);
pub static COLLAPSE_EVENTS:          AtomicU64 = AtomicU64::new(0);

const OVERLOAD_DEPTH:            u32  = 6;
const SCHEDULER_COLLAPSE_LOAD:   f32  = 0.92;
const STARVATION_UNCERTAINTY:    f32  = 0.88;

// ── StabilityReport ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StabilityReport {
    pub recursion_depth:            u32,
    pub has_amplification_risk:     bool,
    pub has_distributed_overload:   bool,
    pub has_scheduler_collapse:     bool,
    pub has_service_starvation:     bool,
    pub has_simulation_runaway:     bool,
    pub overall_stable:             bool,
    pub risk_score:                 f32,
}

impl StabilityReport {
    pub fn is_critical(&self) -> bool { self.risk_score > 0.70 }
}

// ── Check API ─────────────────────────────────────────────────────────────────

pub fn check() -> StabilityReport {
    let depth      = CURRENT_RECURSION_DEPTH.load(Ordering::Relaxed);
    let avg_load   = crate::adaptive_topology::avg_load();
    let unc        = crate::generalized_uncertainty::profile();
    let prob       = crate::probabilistic_stability::check();

    let has_amplification_risk   = depth >= OVERLOAD_DEPTH;
    let has_distributed_overload = avg_load > SCHEDULER_COLLAPSE_LOAD;
    let has_scheduler_collapse   = avg_load > SCHEDULER_COLLAPSE_LOAD
                                  && prob.instability_score > 0.70;
    let has_service_starvation   = unc.overall > STARVATION_UNCERTAINTY;
    let has_simulation_runaway   = !crate::simulation_safety::check_simulation_safe().is_safe
        && avg_load > 0.70;

    if has_amplification_risk {
        AMPLIFICATION_EVENTS.fetch_add(1, Ordering::Relaxed);
    }
    if has_scheduler_collapse || has_distributed_overload {
        COLLAPSE_EVENTS.fetch_add(1, Ordering::Relaxed);
    }

    let risks = [
        has_amplification_risk,
        has_distributed_overload,
        has_scheduler_collapse,
        has_service_starvation,
        has_simulation_runaway,
    ];
    let risk_count = risks.iter().filter(|&&r| r).count();
    let risk_score = (risk_count as f32 / risks.len() as f32)
        + (depth as f32 / MAX_SAFE_RECURSION_DEPTH as f32) * 0.30;
    let risk_score = risk_score.clamp(0.0, 1.0);

    StabilityReport {
        recursion_depth:          depth,
        has_amplification_risk,
        has_distributed_overload,
        has_scheduler_collapse,
        has_service_starvation,
        has_simulation_runaway,
        overall_stable:           risk_score < 0.40,
        risk_score,
    }
}

// ── Depth management ──────────────────────────────────────────────────────────

/// Enter a recursive cognition level. Returns false if max depth exceeded.
pub fn enter_recursion() -> bool {
    let prev = CURRENT_RECURSION_DEPTH.fetch_add(1, Ordering::Relaxed);
    if prev + 1 > MAX_SAFE_RECURSION_DEPTH {
        CURRENT_RECURSION_DEPTH.fetch_sub(1, Ordering::Relaxed);
        AMPLIFICATION_EVENTS.fetch_add(1, Ordering::Relaxed);
        false
    } else {
        true
    }
}

/// Exit a recursive cognition level.
pub fn exit_recursion() {
    CURRENT_RECURSION_DEPTH.fetch_update(
        Ordering::Relaxed, Ordering::Relaxed,
        |d| Some(d.saturating_sub(1))
    ).ok();
}

pub fn current_depth()        -> u32  { CURRENT_RECURSION_DEPTH.load(Ordering::Relaxed) }
pub fn amplification_events() -> u64  { AMPLIFICATION_EVENTS.load(Ordering::Relaxed) }
pub fn collapse_events()      -> u64  { COLLAPSE_EVENTS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_no_panic() {
        let r = check();
        assert!(r.risk_score >= 0.0 && r.risk_score <= 1.0);
    }

    #[test]
    fn enter_exit_recursion_balanced() {
        let ok = enter_recursion();
        if ok { exit_recursion(); }
        assert!(current_depth() <= MAX_SAFE_RECURSION_DEPTH);
    }

    #[test]
    fn depth_never_exceeds_max() {
        for _ in 0..20 { let _ = enter_recursion(); }
        assert!(current_depth() <= MAX_SAFE_RECURSION_DEPTH);
        // Clean up
        for _ in 0..MAX_SAFE_RECURSION_DEPTH {
            exit_recursion();
        }
    }
}

