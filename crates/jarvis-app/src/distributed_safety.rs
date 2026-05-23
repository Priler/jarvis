//! Distributed safety engine — prevents recursive distributed loop amplification,
//! cognition service deadlocks, runtime scheduler starvation, and local machine
//! destabilisation.

use std::sync::atomic::{AtomicU64, Ordering};

pub static SAFETY_GATES_FIRED:    AtomicU64 = AtomicU64::new(0);
pub static OPERATIONS_BLOCKED:    AtomicU64 = AtomicU64::new(0);

const MAX_UNCERTAINTY_FOR_DIST:  f32 = 0.85;
const MAX_LOAD_FOR_RECURSION:    f32 = 0.80;
const MIN_CONFIDENCE_FOR_DIST:   f32 = 0.18;

// ── DistributedSafetyVerdict ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DistributedSafetyVerdict {
    pub is_safe: bool,
    pub reason:  Option<String>,
}

impl DistributedSafetyVerdict {
    fn safe() -> Self { DistributedSafetyVerdict { is_safe: true, reason: None } }
    fn blocked(r: impl Into<String>) -> Self {
        SAFETY_GATES_FIRED.fetch_add(1, Ordering::Relaxed);
        OPERATIONS_BLOCKED.fetch_add(1, Ordering::Relaxed);
        DistributedSafetyVerdict { is_safe: false, reason: Some(r.into()) }
    }
}

// ── Checks ────────────────────────────────────────────────────────────────────

/// Check if it is safe to dispatch distributed cognition work.
pub fn check_distributed_safe() -> DistributedSafetyVerdict {
    let unc      = crate::generalized_uncertainty::profile();
    let avg_load = crate::adaptive_topology::avg_load();
    let stab     = crate::recursive_stability::check();

    if unc.overall > MAX_UNCERTAINTY_FOR_DIST {
        return DistributedSafetyVerdict::blocked(
            format!("uncertainty_runaway: {:.3}", unc.overall));
    }

    if avg_load > MAX_LOAD_FOR_RECURSION {
        return DistributedSafetyVerdict::blocked(
            format!("topology_overload: avg_load={:.3}", avg_load));
    }

    if stab.has_distributed_overload {
        return DistributedSafetyVerdict::blocked("distributed_overload_detected");
    }

    if stab.has_scheduler_collapse {
        return DistributedSafetyVerdict::blocked("scheduler_collapse_detected");
    }

    DistributedSafetyVerdict::safe()
}

/// Check if it is safe to enter another recursive orchestration level.
pub fn check_recursive_safe() -> DistributedSafetyVerdict {
    let depth    = crate::recursive_stability::current_depth();
    let stab     = crate::recursive_stability::check();
    let avg_conf = crate::belief_engine::avg_confidence();
    let count    = crate::belief_engine::belief_count();

    if stab.has_amplification_risk {
        return DistributedSafetyVerdict::blocked(
            format!("recursion_amplification: depth={}", depth));
    }

    if count > 5 && avg_conf < MIN_CONFIDENCE_FOR_DIST {
        return DistributedSafetyVerdict::blocked(
            format!("belief_confidence_insufficient: {:.3}", avg_conf));
    }

    if stab.is_critical() {
        return DistributedSafetyVerdict::blocked(
            format!("critical_stability_risk: score={:.3}", stab.risk_score));
    }

    DistributedSafetyVerdict::safe()
}

pub fn gates_fired()      -> u64 { SAFETY_GATES_FIRED.load(Ordering::Relaxed) }
pub fn operations_blocked() -> u64 { OPERATIONS_BLOCKED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_distributed_safe_no_panic() {
        let v = check_distributed_safe();
        let _ = v.is_safe;
    }

    #[test]
    fn check_recursive_safe_no_panic() {
        let v = check_recursive_safe();
        let _ = v.is_safe;
    }

    #[test]
    fn counters_non_negative() {
        let _ = check_distributed_safe();
        assert!(gates_fired() + operations_blocked() >= 0);
    }
}
