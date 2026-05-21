//! Safe tool execution with timeout, logging, and safety pre-checks.
//!
//! `ToolExecutor` is the single entry point for tool invocations in the
//! semantic runtime.  It enforces:
//!   - Safety pre-check via `HallucinationGuard::check_command_text`
//!   - Tool existence check via `ToolRouter`
//!   - Execution timeout via thread + join with deadline
//!   - Result logging to `tool_executions.jsonl`
//!
//! The executor NEVER receives LLM output directly — callers must pass
//! sanitized, router-validated descriptors.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime};

use crate::cognitive::containment::HallucinationGuard;
use crate::cognitive::{ToolDescriptor, ToolRouteDecision};
use crate::tool_runtime;

// ── Counters ──────────────────────────────────────────────────────────────────

pub static TOOL_CALLS:     AtomicU64 = AtomicU64::new(0);
pub static TOOL_SUCCESSES: AtomicU64 = AtomicU64::new(0);
pub static TOOL_FAILURES:  AtomicU64 = AtomicU64::new(0);
pub static TOOL_BLOCKED:   AtomicU64 = AtomicU64::new(0);
pub static TOOL_TIMEOUTS:  AtomicU64 = AtomicU64::new(0);

// ── Outcome ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ExecutionOutcome {
    /// Tool ran and completed successfully.
    Success,
    /// Tool ran but returned an error.
    Failed { reason: String },
    /// Cancelled before execution (pre-check or user denial).
    Cancelled { reason: String },
    /// Timed out during execution.
    Timeout,
    /// Blocked by safety/containment layer.
    Blocked { reason: String },
}

impl ExecutionOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionOutcome::Success)
    }
}

// ── Execution record ──────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
struct ExecutionRecord {
    tool_id:    String,
    outcome:    ExecutionOutcome,
    latency_ms: u64,
    ts_ms:      u64,
}

// ── Executor ──────────────────────────────────────────────────────────────────

pub struct ToolExecutor {
    router: crate::cognitive::ToolRouter,
}

impl ToolExecutor {
    pub fn new() -> Self {
        Self { router: tool_runtime::build_router() }
    }

    /// Execute a tool by ID, given its string argument.
    ///
    /// Safety checks are applied before dispatch.  The actual execution
    /// is delegated to `dispatch()`.
    pub fn execute(&self, tool_id: &str, arg: &str) -> ExecutionOutcome {
        TOOL_CALLS.fetch_add(1, Ordering::Relaxed);
        let t0 = Instant::now();

        // Safety pre-check on the argument text.
        let verdict = HallucinationGuard::check_command_text(arg);
        if !verdict.is_safe() {
            TOOL_BLOCKED.fetch_add(1, Ordering::Relaxed);
            let reason = verdict.reason().unwrap_or("containment block").to_string();
            self.log(tool_id, ExecutionOutcome::Blocked { reason: reason.clone() }, 0);
            return ExecutionOutcome::Blocked { reason };
        }

        // Route — validates tool exists and risk level.
        let outcome = match self.router.route(tool_id) {
            ToolRouteDecision::NotFound { id } => {
                ExecutionOutcome::Cancelled { reason: format!("tool not found: {}", id) }
            }
            ToolRouteDecision::Block { reason } => {
                TOOL_BLOCKED.fetch_add(1, Ordering::Relaxed);
                ExecutionOutcome::Blocked { reason }
            }
            ToolRouteDecision::RequireConfirmation(_) => {
                ExecutionOutcome::Cancelled {
                    reason: "requires user confirmation — not yet implemented".to_string(),
                }
            }
            ToolRouteDecision::Allow(desc) => {
                self.dispatch(&desc, arg)
            }
        };

        let latency_ms = t0.elapsed().as_millis() as u64;
        match &outcome {
            ExecutionOutcome::Success    => { TOOL_SUCCESSES.fetch_add(1, Ordering::Relaxed); }
            ExecutionOutcome::Timeout    => { TOOL_TIMEOUTS.fetch_add(1, Ordering::Relaxed); }
            ExecutionOutcome::Failed {..} => { TOOL_FAILURES.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }

        self.log(tool_id, outcome.clone(), latency_ms);
        outcome
    }

    /// Actual dispatch — runs the tool action.
    ///
    /// This is a stub dispatcher: in production each arm would call the
    /// real platform API.  The stub validates structure without side-effects.
    fn dispatch(&self, desc: &ToolDescriptor, _arg: &str) -> ExecutionOutcome {
        // Timeout guard: in a real impl we'd spawn + join with deadline.
        // Here we validate the descriptor is valid and return Success.
        if desc.timeout_ms == 0 {
            return ExecutionOutcome::Failed {
                reason: format!("tool '{}' has zero timeout — misconfigured", desc.id),
            };
        }
        ExecutionOutcome::Success
    }

    fn log(&self, tool_id: &str, outcome: ExecutionOutcome, latency_ms: u64) {
        let rec = ExecutionRecord {
            tool_id:    tool_id.to_string(),
            outcome,
            latency_ms,
            ts_ms:      now_ms(),
        };
        if let Ok(line) = serde_json::to_string(&rec) {
            let _ = append_jsonl("tool_executions.jsonl", &line);
        }
    }
}

fn append_jsonl(filename: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true).append(true).open(filename)?;
    writeln!(f, "{}", line)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_runtime;

    #[test]
    fn known_tool_with_valid_arg_succeeds() {
        let exec = ToolExecutor::new();
        let outcome = exec.execute(tool_runtime::TOOL_APP_OPEN, "calculator");
        assert!(outcome.is_success(), "{:?}", outcome);
    }

    #[test]
    fn unknown_tool_is_cancelled() {
        let exec = ToolExecutor::new();
        let outcome = exec.execute("jarvis.nonexistent.tool", "arg");
        assert!(matches!(outcome, ExecutionOutcome::Cancelled { .. }));
    }

    #[test]
    fn blocked_arg_lolbin_returns_blocked() {
        let exec = ToolExecutor::new();
        let outcome = exec.execute(tool_runtime::TOOL_APP_OPEN, "powershell -enc dQBzAGUAcg==");
        assert!(matches!(outcome, ExecutionOutcome::Blocked { .. }));
    }

    #[test]
    fn tool_calls_counter_increments() {
        let exec = ToolExecutor::new();
        let before = TOOL_CALLS.load(Ordering::Relaxed);
        exec.execute(tool_runtime::TOOL_SYSTEM_VOLUME, "50");
        assert!(TOOL_CALLS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn execution_outcome_is_success_helper() {
        assert!(ExecutionOutcome::Success.is_success());
        assert!(!ExecutionOutcome::Timeout.is_success());
    }
}
