//! Execution sandbox — validates tool arguments before dispatch.
//!
//! Provides a second safety layer below `HallucinationGuard`:
//!   - Argument length and character checks
//!   - Path traversal detection
//!   - Scope-based tool restrictions
//!   - System directory access prevention
//!
//! The sandbox never executes tools — it only validates call parameters.

use std::sync::atomic::{AtomicU64, Ordering};
use crate::tool_capabilities::{ToolScope, get as get_profile};

pub static SANDBOX_VALIDATIONS: AtomicU64 = AtomicU64::new(0);
pub static SANDBOX_VIOLATIONS:  AtomicU64 = AtomicU64::new(0);
pub static SANDBOX_PASSED:      AtomicU64 = AtomicU64::new(0);

// ── Violation ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct SandboxViolation {
    pub tool_id: String,
    pub reason:  String,
}

// ── Sandbox ───────────────────────────────────────────────────────────────────

pub struct ExecutionSandbox;

impl ExecutionSandbox {
    /// Validate a tool call against sandbox rules.
    ///
    /// Returns `Ok(())` if the call is safe, or `Err(SandboxViolation)` on the
    /// first violation found.
    pub fn validate(tool_id: &str, arg: &str) -> Result<(), SandboxViolation> {
        SANDBOX_VALIDATIONS.fetch_add(1, Ordering::Relaxed);

        // R1: argument must not be empty.
        if arg.trim().is_empty() {
            return Self::violation(tool_id, "empty argument");
        }

        // R2: argument length limit (256 chars, same as HallucinationGuard).
        if arg.len() > 256 {
            return Self::violation(tool_id, "argument exceeds 256 characters");
        }

        // R3: path traversal prevention.
        if arg.contains("../") || arg.contains("..\\") || arg.contains("%2e%2e") {
            return Self::violation(tool_id, "path traversal detected in argument");
        }

        // R4: system directory access prevention (Windows + Unix).
        let arg_lower = arg.to_lowercase();
        const BLOCKED_PATHS: &[&str] = &[
            "c:\\windows\\system32", "c:/windows/system32",
            "/etc/passwd", "/etc/shadow", "/proc/",
            "c:\\program files", "c:/program files",
        ];
        for blocked in BLOCKED_PATHS {
            if arg_lower.contains(blocked) {
                return Self::violation(tool_id, &format!("blocked path: {}", blocked));
            }
        }

        // R5: ASCII control characters (except tab/newline).
        if arg.chars().any(|c| c.is_control() && c != '\t' && c != '\n') {
            return Self::violation(tool_id, "argument contains control characters");
        }

        // R6: scope-based restriction — tools with LocalKnowledge scope must not
        //     receive arguments that look like external URLs.
        if let Some(profile) = get_profile(tool_id) {
            if profile.scope == ToolScope::LocalKnowledge {
                if arg.starts_with("http://") || arg.starts_with("https://") {
                    return Self::violation(tool_id, "network URL forbidden in offline-only tool");
                }
            }
        }

        SANDBOX_PASSED.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn violation(tool_id: &str, reason: &str) -> Result<(), SandboxViolation> {
        SANDBOX_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
        Err(SandboxViolation {
            tool_id: tool_id.to_string(),
            reason:  reason.to_string(),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_runtime;

    #[test]
    fn valid_arg_passes() {
        let r = ExecutionSandbox::validate(tool_runtime::TOOL_APP_OPEN, "calculator");
        assert!(r.is_ok());
    }

    #[test]
    fn empty_arg_violates() {
        let r = ExecutionSandbox::validate(tool_runtime::TOOL_APP_OPEN, "  ");
        assert!(r.is_err());
    }

    #[test]
    fn path_traversal_violates() {
        let r = ExecutionSandbox::validate(tool_runtime::TOOL_APP_OPEN, "../../etc/passwd");
        assert!(r.is_err());
    }

    #[test]
    fn system32_path_violates() {
        let r = ExecutionSandbox::validate(tool_runtime::TOOL_APP_OPEN, "C:\\Windows\\System32\\cmd.exe");
        assert!(r.is_err());
    }

    #[test]
    fn url_in_knowledge_query_violates() {
        let r = ExecutionSandbox::validate(tool_runtime::TOOL_INFO_QUERY, "https://evil.com/payload");
        assert!(r.is_err());
    }

    #[test]
    fn sandbox_violations_counter_increments() {
        let before = SANDBOX_VIOLATIONS.load(Ordering::Relaxed);
        ExecutionSandbox::validate(tool_runtime::TOOL_APP_OPEN, "").ok();
        assert!(SANDBOX_VIOLATIONS.load(Ordering::Relaxed) > before);
    }
}
