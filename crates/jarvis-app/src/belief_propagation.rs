//! Belief propagation — propagates confidence, uncertainty, and degradation
//! risk through the uncertainty graph until convergence or max iterations.

use std::sync::atomic::{AtomicU64, Ordering};

pub static PROPAGATION_RUNS:  AtomicU64 = AtomicU64::new(0);
pub static NODES_UPDATED:     AtomicU64 = AtomicU64::new(0);

const MAX_ITERS:             usize = 20;
const CONVERGENCE_THRESHOLD: f32   = 0.01;

// ── PropagationResult ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PropagationResult {
    pub iters_run:     usize,
    pub nodes_updated: usize,
    pub converged:     bool,
    pub final_avg_uncertainty: f32,
}

// ── Propagation ───────────────────────────────────────────────────────────────

pub fn propagate(n_steps: usize) -> PropagationResult {
    let steps = n_steps.min(MAX_ITERS);
    let before = crate::uncertainty_graph::avg_uncertainty();

    crate::uncertainty_graph::propagate(steps);

    let after   = crate::uncertainty_graph::avg_uncertainty();
    let delta   = (after - before).abs();
    let converged = delta < CONVERGENCE_THRESHOLD;

    PROPAGATION_RUNS.fetch_add(1, Ordering::Relaxed);
    let n = crate::uncertainty_graph::node_count();
    NODES_UPDATED.fetch_add(n as u64, Ordering::Relaxed);

    crate::probabilistic_observability::log(
        crate::probabilistic_observability::ProbabilisticEvent::ConfidencePropagated {
            from:  "uncertainty_graph".into(),
            to:    "all_nodes".into(),
            delta,
        }
    );

    PropagationResult {
        iters_run:             steps,
        nodes_updated:         n,
        converged,
        final_avg_uncertainty: after,
    }
}

pub fn propagate_until_convergence() -> PropagationResult {
    propagate(MAX_ITERS)
}

/// Seed the uncertainty graph from current belief engine state.
pub fn seed_from_beliefs() {
    for belief in crate::belief_engine::all_beliefs() {
        crate::uncertainty_graph::upsert_node(&belief.label, belief.confidence);
    }
}

/// Seed uncertainty graph from symbolic inference chains.
pub fn seed_from_chains() {
    for chain in crate::symbolic_inference::reliable_chains() {
        crate::uncertainty_graph::upsert_node(&chain.root,       chain.confidence);
        crate::uncertainty_graph::upsert_node(&chain.conclusion, chain.confidence * 0.90);
        crate::uncertainty_graph::add_dependency(&chain.root, &chain.conclusion, chain.confidence);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propagate_no_panic() {
        let r = propagate(3);
        assert!(r.final_avg_uncertainty >= 0.0 && r.final_avg_uncertainty <= 1.0);
    }

    #[test]
    fn seed_from_beliefs_no_panic() {
        seed_from_beliefs();
    }
}
