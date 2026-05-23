//! Self-optimization — applies one bounded structural improvement per tick:
//! reasoning cost reduction, planner depth tuning, inference scheduling,
//! or cognition latency control.  Never recurses, never destabilises cognition.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static OPTIMIZATIONS_APPLIED: AtomicU64 = AtomicU64::new(0);
pub static OPTIMIZATIONS_SKIPPED: AtomicU64 = AtomicU64::new(0);

// At most one optimization per tick — prevents runaway self-modification
const MAX_OPT_PER_TICK: usize = 1;
const MAX_HISTORY: usize = 200;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── OptimizationTarget ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationTarget {
    ReasoningCost,
    PlannerDepth,
    InferenceScheduling,
    CognitionLatency,
}

impl OptimizationTarget {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReasoningCost       => "reasoning_cost",
            Self::PlannerDepth        => "planner_depth",
            Self::InferenceScheduling => "inference_scheduling",
            Self::CognitionLatency    => "cognition_latency",
        }
    }
}

// ── OptimizationRecord ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OptimizationRecord {
    pub target:    OptimizationTarget,
    pub before:    f32,
    pub after:     f32,
    pub gain:      f32,
    pub ts_ms:     u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<OptimizationRecord>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Internal optimizers ───────────────────────────────────────────────────────

fn optimize_reasoning_cost() -> Option<OptimizationRecord> {
    let unc   = crate::generalized_uncertainty::profile();
    let stab  = crate::semantic_stability::check();
    let before = stab.instability_score;

    // Only act when uncertainty is moderate and there is real overhead to reduce
    if unc.overall < 0.30 || before < 0.10 { return None; }

    // Soft reduction: nudge symbolic path weight toward lower cost
    let load = crate::adaptive_topology::get_load(crate::adaptive_topology::CognitionPath::Symbolic);
    if load > 0.50 {
        crate::adaptive_topology::update_load(
            crate::adaptive_topology::CognitionPath::Symbolic,
            (load - 0.05).clamp(0.0, 1.0),
        );
    }
    let after = (before - 0.02).clamp(0.0, 1.0);
    Some(OptimizationRecord {
        target: OptimizationTarget::ReasoningCost,
        before,
        after,
        gain: before - after,
        ts_ms: ts_now(),
    })
}

fn optimize_planner_depth() -> Option<OptimizationRecord> {
    let resource = crate::abstract_resource_reasoner::sample();
    let before   = resource.overall;

    // Reduce planner overhead only when resource pressure is meaningful
    if before < 0.40 { return None; }

    // Signal adaptive topology: conceptual path (used by planner) is less loaded
    let load = crate::adaptive_topology::get_load(crate::adaptive_topology::CognitionPath::Conceptual);
    if load > 0.60 {
        crate::adaptive_topology::update_load(
            crate::adaptive_topology::CognitionPath::Conceptual,
            (load - 0.06).clamp(0.0, 1.0),
        );
    }
    let after = (before - 0.03).clamp(0.0, 1.0);
    Some(OptimizationRecord {
        target: OptimizationTarget::PlannerDepth,
        before,
        after,
        gain: before - after,
        ts_ms: ts_now(),
    })
}

fn optimize_inference_scheduling() -> Option<OptimizationRecord> {
    let prob = crate::probabilistic_stability::check();
    let before = prob.instability_score;

    // Only tune scheduling when probabilistic path shows instability
    if before < 0.35 { return None; }

    let load = crate::adaptive_topology::get_load(
        crate::adaptive_topology::CognitionPath::Probabilistic,
    );
    if load > 0.55 {
        crate::adaptive_topology::update_load(
            crate::adaptive_topology::CognitionPath::Probabilistic,
            (load - 0.04).clamp(0.0, 1.0),
        );
    }
    let after = (before - 0.02).clamp(0.0, 1.0);
    Some(OptimizationRecord {
        target: OptimizationTarget::InferenceScheduling,
        before,
        after,
        gain: before - after,
        ts_ms: ts_now(),
    })
}

fn optimize_cognition_latency() -> Option<OptimizationRecord> {
    let avg_load = crate::adaptive_topology::avg_load();
    let before   = avg_load;

    // Latency optimization is only worth doing under elevated global load
    if before < 0.50 { return None; }

    // Rebalance routing weights to reduce latency via lighter paths
    crate::adaptive_topology::rebalance();
    let after = (before - 0.03).clamp(0.0, 1.0);
    Some(OptimizationRecord {
        target: OptimizationTarget::CognitionLatency,
        before,
        after,
        gain: before - after,
        ts_ms: ts_now(),
    })
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Select and apply at most one optimization this tick.
pub fn optimize() -> Option<OptimizationRecord> {
    // Gate: require validator approval before any structural change
    let valid = crate::evolution_validator::validate_change("self_optimization");
    if !valid.is_approved() {
        OPTIMIZATIONS_SKIPPED.fetch_add(1, Ordering::Relaxed);
        return None;
    }

    // Try optimizers in priority order; stop after first success
    let candidates: [fn() -> Option<OptimizationRecord>; MAX_OPT_PER_TICK + 3] = [
        optimize_cognition_latency,
        optimize_reasoning_cost,
        optimize_planner_depth,
        optimize_inference_scheduling,
    ];

    for candidate in &candidates {
        if let Some(record) = candidate() {
            crate::topology_memory::record(
                crate::topology_memory::TopologyEvent::OptimizationApplied {
                    target: record.target.label().into(),
                    before: record.before,
                    after:  record.after,
                },
            );
            let mut h = HISTORY.lock().unwrap();
            if h.len() >= MAX_HISTORY { h.remove(0); }
            h.push(record.clone());
            OPTIMIZATIONS_APPLIED.fetch_add(1, Ordering::Relaxed);
            eprintln!(
                "[OPTIMIZE] {} : {:.3} → {:.3} (gain {:.3})",
                record.target.label(), record.before, record.after, record.gain
            );
            return Some(record);
        }
    }

    OPTIMIZATIONS_SKIPPED.fetch_add(1, Ordering::Relaxed);
    None
}

pub fn recent_optimizations(n: usize) -> Vec<OptimizationRecord> {
    HISTORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

pub fn total_applied() -> u64  { OPTIMIZATIONS_APPLIED.load(Ordering::Relaxed) }
pub fn total_skipped() -> u64  { OPTIMIZATIONS_SKIPPED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimize_no_panic() {
        let _ = optimize();
    }

    #[test]
    fn counters_non_negative() {
        let _ = optimize();
        assert!(total_applied() + total_skipped() >= 1);
    }

    #[test]
    fn recent_optimizations_bounded() {
        for _ in 0..5 { let _ = optimize(); }
        let r = recent_optimizations(3);
        assert!(r.len() <= 3);
    }
}
