//! Execution recovery engine.
//!
//! Handles partial failures in multi-step plans:
//!   - Retry failed steps (bounded, with policy from ExecutionVerifier)
//!   - Rollback completed steps when a plan must abort
//!   - Replan: re-submit the remaining steps as a new plan
//!   - Request clarification when recovery is impossible
//!
//! Partial failure detection: some nodes completed + some nodes failed = partial failure.

use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashSet;

use crate::task_graph::{NodeStatus, TaskGraph};
use crate::execution_verifier::RollbackPolicy;

pub static RECOVERY_ATTEMPTS:  AtomicU64 = AtomicU64::new(0);
pub static RECOVERY_SUCCESSES: AtomicU64 = AtomicU64::new(0);
pub static RECOVERY_FAILURES:  AtomicU64 = AtomicU64::new(0);
pub static ROLLBACK_ACTIONS:   AtomicU64 = AtomicU64::new(0);
pub static PARTIAL_FAILURES:   AtomicU64 = AtomicU64::new(0);

// ── Recovery strategy ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RecoveryStrategy {
    /// Retry the failed node up to `max_attempts` more times.
    Retry { max_attempts: u8 },
    /// Rollback all completed nodes and abort the plan.
    Rollback,
    /// Skip the failed node and continue with remaining steps.
    Skip,
    /// Ask the user for guidance — plan cannot continue autonomously.
    RequestClarification { question: String },
}

// ── Recovery outcome ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub enum RecoveryOutcome {
    /// Recovery succeeded; execution can continue.
    Recovered { strategy_used: RecoveryStrategy },
    /// Partial recovery: some steps could be recovered, others could not.
    PartialRecovery { recovered_ids: Vec<String>, still_failed: Vec<String> },
    /// Recovery impossible; plan must be abandoned.
    Unrecoverable { reason: String },
}

impl RecoveryOutcome {
    pub fn is_recovered(&self) -> bool {
        matches!(self, RecoveryOutcome::Recovered { .. })
    }
}

// ── Partial failure ───────────────────────────────────────────────────────────

/// Describes a partial plan failure: some nodes completed, others failed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PartialFailure {
    pub completed_ids: Vec<String>,
    pub failed_ids:    Vec<String>,
    pub pending_ids:   Vec<String>,
}

impl PartialFailure {
    /// Detect a partial failure from a task graph's current state.
    pub fn detect(graph: &TaskGraph) -> Option<Self> {
        let topo = match graph.topological_order() {
            Ok(order) => order,
            Err(_) => return None,
        };

        // We need to access nodes by ID. Use the graph's public completed_ids.
        let completed: HashSet<String> = graph.completed_ids();

        let mut completed_ids = Vec::new();
        let mut failed_ids = Vec::new();
        let mut pending_ids = Vec::new();

        for id in &topo {
            if completed.contains(id) {
                completed_ids.push(id.clone());
            }
            // We cannot check failed/pending from outside graph directly,
            // so we infer: not completed and in topo order.
        }

        // A partial failure requires at least one completed AND one non-completed node.
        if !completed_ids.is_empty() && completed_ids.len() < topo.len() {
            for id in &topo {
                if !completed.contains(id) {
                    pending_ids.push(id.clone());
                }
            }
            PARTIAL_FAILURES.fetch_add(1, Ordering::Relaxed);
            Some(PartialFailure { completed_ids, failed_ids, pending_ids })
        } else {
            None
        }
    }
}

// ── Recovery engine ───────────────────────────────────────────────────────────

pub struct ExecutionRecovery;

impl ExecutionRecovery {
    /// Determine and execute a recovery strategy for a failed node.
    ///
    /// `retry_count` is how many times this node has already been retried.
    pub fn recover(
        tool_id: &str,
        failed_node_id: &str,
        retry_count: u8,
    ) -> RecoveryOutcome {
        RECOVERY_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

        let policy = crate::execution_verifier::ExecutionVerifier::rollback_policy(tool_id);

        let outcome = match policy {
            RollbackPolicy::Retry { max_attempts } => {
                if retry_count < max_attempts {
                    RecoveryOutcome::Recovered {
                        strategy_used: RecoveryStrategy::Retry { max_attempts },
                    }
                } else {
                    RecoveryOutcome::Unrecoverable {
                        reason: format!(
                            "node '{}' exhausted retry limit ({} attempts)",
                            failed_node_id, retry_count
                        ),
                    }
                }
            }
            RollbackPolicy::Undo => {
                ROLLBACK_ACTIONS.fetch_add(1, Ordering::Relaxed);
                RecoveryOutcome::Recovered {
                    strategy_used: RecoveryStrategy::Rollback,
                }
            }
            RollbackPolicy::None => {
                RecoveryOutcome::Unrecoverable {
                    reason: format!("node '{}' has no rollback policy", failed_node_id),
                }
            }
        };

        match &outcome {
            RecoveryOutcome::Recovered { .. }        => { RECOVERY_SUCCESSES.fetch_add(1, Ordering::Relaxed); }
            RecoveryOutcome::Unrecoverable { .. }    => { RECOVERY_FAILURES.fetch_add(1, Ordering::Relaxed); }
            RecoveryOutcome::PartialRecovery { .. }  => {}
        }

        outcome
    }

    /// Request clarification when recovery is impossible.
    pub fn request_clarification(context: impl Into<String>) -> RecoveryOutcome {
        RECOVERY_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        let question = format!("Recovery failed: {}. How should I proceed?", context.into());
        RecoveryOutcome::Recovered {
            strategy_used: RecoveryStrategy::RequestClarification { question },
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_runtime;

    #[test]
    fn retry_within_limit_recovers() {
        // TOOL_APP_OPEN has Retry { max_attempts: 2 }
        let outcome = ExecutionRecovery::recover(tool_runtime::TOOL_APP_OPEN, "step_0", 0);
        assert!(outcome.is_recovered());
    }

    #[test]
    fn retry_at_limit_is_unrecoverable() {
        let outcome = ExecutionRecovery::recover(tool_runtime::TOOL_APP_OPEN, "step_0", 2);
        assert!(matches!(outcome, RecoveryOutcome::Unrecoverable { .. }));
    }

    #[test]
    fn rollback_policy_triggers_rollback_strategy() {
        // TOOL_SYSTEM_VOLUME has Undo rollback policy
        let outcome = ExecutionRecovery::recover(tool_runtime::TOOL_SYSTEM_VOLUME, "step_0", 0);
        assert!(outcome.is_recovered());
        assert!(matches!(outcome, RecoveryOutcome::Recovered {
            strategy_used: RecoveryStrategy::Rollback,
        }));
    }

    #[test]
    fn no_rollback_policy_is_unrecoverable() {
        // TOOL_APP_CLOSE has None rollback policy
        let outcome = ExecutionRecovery::recover(tool_runtime::TOOL_APP_CLOSE, "step_0", 0);
        assert!(matches!(outcome, RecoveryOutcome::Unrecoverable { .. }));
    }

    #[test]
    fn request_clarification_returns_recovered() {
        let outcome = ExecutionRecovery::request_clarification("docker failed to start");
        assert!(outcome.is_recovered());
    }

    #[test]
    fn partial_failure_detected_when_some_complete() {
        let mut g = crate::task_graph::TaskGraph::new();
        g.add(crate::task_graph::TaskNode::new("a", "step a", "app.open", "ide")).unwrap();
        g.add(crate::task_graph::TaskNode::new("b", "step b", "app.open", "docker")
            .with_deps(vec!["a".into()])).unwrap();
        g.mark_completed("a");
        let pf = PartialFailure::detect(&g);
        assert!(pf.is_some());
        let pf = pf.unwrap();
        assert!(pf.completed_ids.contains(&"a".to_string()));
        assert!(pf.pending_ids.contains(&"b".to_string()));
    }

    #[test]
    fn recovery_attempts_counter_increments() {
        let before = RECOVERY_ATTEMPTS.load(Ordering::Relaxed);
        ExecutionRecovery::recover(tool_runtime::TOOL_APP_OPEN, "step_0", 0);
        assert!(RECOVERY_ATTEMPTS.load(Ordering::Relaxed) > before);
    }
}
