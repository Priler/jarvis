#![allow(dead_code)]

//! Execution graph — sequential traversal of a validated PlanGraph.
//!
//! The executor NEVER plans; it only traverses a `PlanExecutionBoundary`
//! produced by the planner. The LLM is not involved in execution.

use std::time::SystemTime;
use super::planner::{PlanExecutionBoundary, PlanStep, StepStatus, EdgeKind};
use super::containment::{HallucinationGuard, ContainmentVerdict};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Step executor trait ───────────────────────────────────────────────────────

/// Pluggable backend that runs a single plan step.
///
/// Implementors bridge the execution graph to the actual runtime
/// (e.g., `execute_command` in `app.rs`). They must not perform any
/// planning or LLM inference.
pub trait StepExecutor: Send + Sync {
    /// Execute one plan step. Return `true` on success.
    fn execute_step(&self, step: &PlanStep) -> bool;
}

// ── Execution report ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub step_index: usize,
    pub command_text: String,
    pub success: bool,
    pub latency_ms: u64,
    /// Set when containment blocked this step.
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub goal: String,
    pub total_steps: usize,
    pub outcomes: Vec<StepOutcome>,
    pub aborted: bool,
    pub start_ms: u64,
    pub end_ms: u64,
}

impl ExecutionReport {
    pub fn succeeded_count(&self) -> usize {
        self.outcomes.iter().filter(|o| o.success).count()
    }

    pub fn failed_count(&self) -> usize {
        self.outcomes.iter().filter(|o| !o.success).count()
    }

    pub fn blocked_count(&self) -> usize {
        self.outcomes.iter().filter(|o| o.blocked_reason.is_some()).count()
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    pub fn all_succeeded(&self) -> bool {
        !self.aborted && self.succeeded_count() == self.total_steps
    }
}

// ── Execution engine ─────────────────────────────────────────────────────────

/// Traverses a `PlanExecutionBoundary` sequentially.
///
/// Each step is containment-checked before the executor is called.
/// A blocked or failed step aborts the remainder (fail-fast).
pub struct ExecutionEngine<'a> {
    executor: &'a dyn StepExecutor,
}

impl<'a> ExecutionEngine<'a> {
    pub fn new(executor: &'a dyn StepExecutor) -> Self {
        Self { executor }
    }

    /// Run all steps in sequential order, obeying edge conditions.
    /// Returns a full `ExecutionReport`.
    pub fn run_sequential(&self, boundary: &PlanExecutionBoundary) -> ExecutionReport {
        let graph = &boundary.graph;
        let start_ms = now_ms();
        let mut outcomes: Vec<StepOutcome> = Vec::new();
        let mut aborted = false;

        // Start from node 0; follow Sequential/OnSuccess/OnFailure edges.
        let mut current: Option<usize> = if graph.nodes.is_empty() { None } else { Some(0) };

        while let Some(idx) = current {
            let step = &graph.nodes[idx];
            let step_start = now_ms();

            // Containment check before execution.
            let verdict = HallucinationGuard::check_command_text(&step.command_text);
            if !verdict.is_safe() {
                let reason = verdict.reason().unwrap_or("blocked").to_string();
                warn!("[EXEC_ENGINE] Step {} blocked by containment: {}", idx, reason);
                outcomes.push(StepOutcome {
                    step_index: idx,
                    command_text: step.command_text.clone(),
                    success: false,
                    latency_ms: now_ms().saturating_sub(step_start),
                    blocked_reason: Some(reason),
                });
                aborted = true;
                break;
            }

            let success = self.executor.execute_step(step);
            let latency_ms = now_ms().saturating_sub(step_start);

            debug!("[EXEC_ENGINE] Step {} '{}' success={} latency={}ms",
                idx, step.command_text, success, latency_ms);

            outcomes.push(StepOutcome {
                step_index: idx,
                command_text: step.command_text.clone(),
                success,
                latency_ms,
                blocked_reason: None,
            });

            // Abort on failure (no recovery edges implemented yet).
            if !success {
                // Check for an OnFailure edge before aborting.
                let recovery = graph.edges.iter()
                    .find(|e| e.from == idx && e.kind == EdgeKind::OnFailure)
                    .map(|e| e.to);

                if let Some(rec_idx) = recovery {
                    current = Some(rec_idx);
                    continue;
                }

                aborted = true;
                break;
            }

            // Follow Sequential or OnSuccess edges.
            current = graph.edges.iter()
                .find(|e| e.from == idx && matches!(e.kind, EdgeKind::Sequential | EdgeKind::OnSuccess))
                .map(|e| e.to);
        }

        ExecutionReport {
            goal: graph.goal.clone(),
            total_steps: graph.nodes.len(),
            outcomes,
            aborted,
            start_ms,
            end_ms: now_ms(),
        }
    }
}

// ── Null executor (for tests / dry-run) ──────────────────────────────────────

pub struct NullExecutor;

impl StepExecutor for NullExecutor {
    fn execute_step(&self, _step: &PlanStep) -> bool { true }
}

pub struct FailingExecutor;

impl StepExecutor for FailingExecutor {
    fn execute_step(&self, _step: &PlanStep) -> bool { false }
}
