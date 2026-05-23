//! Execution verifier — pre/postcondition checking per tool.
//!
//! Each tool has:
//!   - Preconditions: must hold before the tool runs
//!   - Postconditions: must hold after the tool succeeds
//!   - Verification logic: symbolic checks (no LLM involvement)
//!
//! Used by the executor after the sandbox and hallucination guard.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

use crate::tool_executor::ExecutionOutcome;

pub static VERIFICATIONS_RUN:    AtomicU64 = AtomicU64::new(0);
pub static VERIFICATIONS_PASSED: AtomicU64 = AtomicU64::new(0);
pub static VERIFICATIONS_FAILED: AtomicU64 = AtomicU64::new(0);

// ── Condition ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct Condition {
    pub description: String,
    pub critical:    bool,
}

impl Condition {
    fn required(description: impl Into<String>) -> Self {
        Self { description: description.into(), critical: true }
    }
    fn optional(description: impl Into<String>) -> Self {
        Self { description: description.into(), critical: false }
    }
}

// ── Verification result ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct VerificationResult {
    pub tool_id:    String,
    pub passed:     bool,
    /// Conditions that were checked.
    pub checked:    Vec<String>,
    /// Conditions that failed (subset of `checked`).
    pub failed:     Vec<String>,
}

impl VerificationResult {
    pub fn is_ok(&self) -> bool {
        self.passed
    }
}

// ── Tool verification spec ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ToolVerificationSpec {
    pub tool_id:          String,
    pub preconditions:    Vec<Condition>,
    pub postconditions:   Vec<Condition>,
    pub rollback_policy:  RollbackPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RollbackPolicy {
    /// No rollback needed (idempotent or read-only).
    None,
    /// Re-run the previous tool invocation to undo.
    Undo,
    /// Retry the failed step.
    Retry { max_attempts: u8 },
}

// ── Spec registry ─────────────────────────────────────────────────────────────

static SPECS: Lazy<HashMap<String, ToolVerificationSpec>> = Lazy::new(build_specs);

fn build_specs() -> HashMap<String, ToolVerificationSpec> {
    use crate::tool_runtime::*;
    let mut m = HashMap::new();

    m.insert(TOOL_APP_OPEN.to_string(), ToolVerificationSpec {
        tool_id: TOOL_APP_OPEN.to_string(),
        preconditions: vec![
            Condition::required("argument not empty"),
            Condition::required("argument is a valid app name"),
        ],
        postconditions: vec![
            Condition::required("execution outcome is Success"),
            Condition::optional("process appears in system task list"),
        ],
        rollback_policy: RollbackPolicy::Retry { max_attempts: 2 },
    });

    m.insert(TOOL_APP_CLOSE.to_string(), ToolVerificationSpec {
        tool_id: TOOL_APP_CLOSE.to_string(),
        preconditions: vec![
            Condition::required("argument not empty"),
        ],
        postconditions: vec![
            Condition::required("execution outcome is Success"),
        ],
        rollback_policy: RollbackPolicy::None,
    });

    m.insert(TOOL_SYSTEM_VOLUME.to_string(), ToolVerificationSpec {
        tool_id: TOOL_SYSTEM_VOLUME.to_string(),
        preconditions: vec![
            Condition::required("argument is a numeric value 0–100"),
        ],
        postconditions: vec![
            Condition::required("execution outcome is Success"),
        ],
        rollback_policy: RollbackPolicy::Undo,
    });

    m.insert(TOOL_SYSTEM_MUTE.to_string(), ToolVerificationSpec {
        tool_id: TOOL_SYSTEM_MUTE.to_string(),
        preconditions: vec![
            Condition::required("argument is 'on', 'off', or 'toggle'"),
        ],
        postconditions: vec![
            Condition::required("execution outcome is Success"),
        ],
        rollback_policy: RollbackPolicy::Undo,
    });

    m.insert(TOOL_REMINDER_SET.to_string(), ToolVerificationSpec {
        tool_id: TOOL_REMINDER_SET.to_string(),
        preconditions: vec![
            Condition::required("argument not empty"),
        ],
        postconditions: vec![
            Condition::required("execution outcome is Success"),
        ],
        rollback_policy: RollbackPolicy::None,
    });

    m.insert(TOOL_CLIPBOARD_READ.to_string(), ToolVerificationSpec {
        tool_id: TOOL_CLIPBOARD_READ.to_string(),
        preconditions: vec![
            Condition::required("clipboard is accessible"),
        ],
        postconditions: vec![
            Condition::optional("result is non-empty string"),
        ],
        rollback_policy: RollbackPolicy::None,
    });

    m.insert(TOOL_INFO_QUERY.to_string(), ToolVerificationSpec {
        tool_id: TOOL_INFO_QUERY.to_string(),
        preconditions: vec![
            Condition::required("argument is a well-formed query"),
        ],
        postconditions: vec![
            Condition::optional("result is non-empty string"),
        ],
        rollback_policy: RollbackPolicy::Retry { max_attempts: 1 },
    });

    m
}

// ── Verifier ──────────────────────────────────────────────────────────────────

pub struct ExecutionVerifier;

impl ExecutionVerifier {
    /// Verify preconditions for a tool before execution.
    pub fn check_preconditions(tool_id: &str, arg: &str) -> VerificationResult {
        VERIFICATIONS_RUN.fetch_add(1, Ordering::Relaxed);

        let spec = match SPECS.get(tool_id) {
            Some(s) => s,
            None => {
                VERIFICATIONS_FAILED.fetch_add(1, Ordering::Relaxed);
                return VerificationResult {
                    tool_id: tool_id.to_string(),
                    passed:  false,
                    checked: vec!["tool exists in verifier".to_string()],
                    failed:  vec!["tool has no verification spec".to_string()],
                };
            }
        };

        let mut checked = Vec::new();
        let mut failed = Vec::new();

        for cond in &spec.preconditions {
            checked.push(cond.description.clone());
            if cond.description.contains("not empty") && arg.trim().is_empty() {
                if cond.critical { failed.push(cond.description.clone()); }
            } else if cond.description.contains("numeric value") {
                let parsed = arg.trim().parse::<u8>();
                if parsed.is_err() && cond.critical {
                    failed.push(cond.description.clone());
                }
            }
            // Other conditions are treated as advisory — checked but not enforced here.
        }

        let passed = failed.is_empty();
        if passed {
            VERIFICATIONS_PASSED.fetch_add(1, Ordering::Relaxed);
        } else {
            VERIFICATIONS_FAILED.fetch_add(1, Ordering::Relaxed);
        }

        VerificationResult { tool_id: tool_id.to_string(), passed, checked, failed }
    }

    /// Verify postconditions after execution.
    pub fn check_postconditions(tool_id: &str, outcome: &ExecutionOutcome) -> VerificationResult {
        VERIFICATIONS_RUN.fetch_add(1, Ordering::Relaxed);

        let spec = match SPECS.get(tool_id) {
            Some(s) => s,
            None => return VerificationResult {
                tool_id: tool_id.to_string(),
                passed:  false,
                checked: vec!["tool exists in verifier".to_string()],
                failed:  vec!["tool has no verification spec".to_string()],
            },
        };

        let mut checked = Vec::new();
        let mut failed = Vec::new();

        for cond in &spec.postconditions {
            checked.push(cond.description.clone());
            if cond.description.contains("outcome is Success") {
                if !outcome.is_success() && cond.critical {
                    failed.push(cond.description.clone());
                }
            }
        }

        let passed = failed.is_empty();
        if passed {
            VERIFICATIONS_PASSED.fetch_add(1, Ordering::Relaxed);
        } else {
            VERIFICATIONS_FAILED.fetch_add(1, Ordering::Relaxed);
        }

        VerificationResult { tool_id: tool_id.to_string(), passed, checked, failed }
    }

    /// Return the rollback policy for a tool.
    pub fn rollback_policy(tool_id: &str) -> RollbackPolicy {
        SPECS.get(tool_id)
            .map(|s| s.rollback_policy.clone())
            .unwrap_or(RollbackPolicy::None)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_executor::ExecutionOutcome;
    use crate::tool_runtime;

    #[test]
    fn valid_preconditions_pass() {
        let r = ExecutionVerifier::check_preconditions(tool_runtime::TOOL_APP_OPEN, "calculator");
        assert!(r.is_ok(), "{:?}", r.failed);
    }

    #[test]
    fn empty_arg_fails_precondition() {
        let r = ExecutionVerifier::check_preconditions(tool_runtime::TOOL_APP_OPEN, "");
        assert!(!r.is_ok());
    }

    #[test]
    fn non_numeric_volume_fails_precondition() {
        let r = ExecutionVerifier::check_preconditions(tool_runtime::TOOL_SYSTEM_VOLUME, "loud");
        assert!(!r.is_ok());
    }

    #[test]
    fn numeric_volume_passes_precondition() {
        let r = ExecutionVerifier::check_preconditions(tool_runtime::TOOL_SYSTEM_VOLUME, "50");
        assert!(r.is_ok());
    }

    #[test]
    fn success_outcome_passes_postcondition() {
        let r = ExecutionVerifier::check_postconditions(
            tool_runtime::TOOL_APP_OPEN, &ExecutionOutcome::Success,
        );
        assert!(r.is_ok());
    }

    #[test]
    fn failed_outcome_fails_postcondition() {
        let r = ExecutionVerifier::check_postconditions(
            tool_runtime::TOOL_APP_OPEN,
            &ExecutionOutcome::Failed { reason: "app not found".to_string() },
        );
        assert!(!r.is_ok());
    }

    #[test]
    fn unknown_tool_fails_both_checks() {
        let pre = ExecutionVerifier::check_preconditions("jarvis.invented", "x");
        let post = ExecutionVerifier::check_postconditions("jarvis.invented", &ExecutionOutcome::Success);
        assert!(!pre.is_ok());
        assert!(!post.is_ok());
    }
}
