//! Security policies — defines enforcement rules for tool execution.
//! Policies escalate from Relaxed → Standard → Strict.

use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;

pub static POLICY_CHECKS:    AtomicU64 = AtomicU64::new(0);
pub static POLICY_BLOCKED:   AtomicU64 = AtomicU64::new(0);
pub static POLICY_VIOLATIONS: AtomicU64 = AtomicU64::new(0);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum SecurityPolicy { Strict, Standard, Relaxed }

#[derive(Debug, Clone, serde::Serialize)]
pub struct PolicyViolation {
    pub tool:    String,
    pub arg:     String,
    pub reason:  &'static str,
    pub ts_ms:   u64,
}

// Dangerous command patterns — always blocked regardless of policy
static DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf", "format c:", "del /f /s", "rd /s /q",
    "shutdown", "reboot", ":(){:|:&};:", "mkfs",
    "dd if=/dev/zero", "chmod 777 /", "> /dev/sda",
];

// System directories — blocked in Strict and Standard
static PROTECTED_PATHS: &[&str] = &[
    "C:\\Windows\\System32", "C:\\Windows\\SysWOW64",
    "/etc/passwd", "/etc/shadow", "/boot", "/proc/sys",
];

struct PolicyState {
    active:     SecurityPolicy,
    violations: Vec<PolicyViolation>,
}

static STATE: Lazy<Mutex<PolicyState>> = Lazy::new(|| Mutex::new(PolicyState {
    active:     SecurityPolicy::Standard,
    violations: Vec::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn set_policy(p: SecurityPolicy) {
    STATE.lock().unwrap().active = p;
}

pub fn get_policy() -> SecurityPolicy {
    STATE.lock().unwrap().active.clone()
}

/// Check if a tool call is permitted under the active policy.
pub fn check(tool: &str, arg: &str) -> Result<(), PolicyViolation> {
    POLICY_CHECKS.fetch_add(1, Ordering::Relaxed);
    let policy = { STATE.lock().unwrap().active.clone() };
    let arg_lower = arg.to_lowercase();

    // Always-dangerous patterns
    for pattern in DANGEROUS_PATTERNS {
        if arg_lower.contains(pattern) {
            let v = record_violation(tool, arg, "dangerous_command_pattern");
            return Err(v);
        }
    }

    match policy {
        SecurityPolicy::Strict => {
            // Strict: block protected paths + require permission for any file/exec
            for path in PROTECTED_PATHS {
                if arg_lower.contains(&path.to_lowercase()) {
                    let v = record_violation(tool, arg, "protected_path_strict");
                    return Err(v);
                }
            }
            if tool.contains("exec") || tool.contains("terminal") {
                let v = record_violation(tool, arg, "exec_blocked_strict_mode");
                return Err(v);
            }
        }
        SecurityPolicy::Standard => {
            for path in PROTECTED_PATHS {
                if arg_lower.contains(&path.to_lowercase()) {
                    let v = record_violation(tool, arg, "protected_path_standard");
                    return Err(v);
                }
            }
        }
        SecurityPolicy::Relaxed => {}
    }

    Ok(())
}

fn record_violation(tool: &str, arg: &str, reason: &'static str) -> PolicyViolation {
    POLICY_BLOCKED.fetch_add(1, Ordering::Relaxed);
    POLICY_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
    let v = PolicyViolation {
        tool:   tool.to_string(),
        arg:    arg[..arg.len().min(200)].to_string(),
        reason,
        ts_ms:  ts_now(),
    };
    let mut s = STATE.lock().unwrap();
    if s.violations.len() >= 500 { s.violations.remove(0); }
    s.violations.push(v.clone());
    v
}

pub fn recent_violations(n: usize) -> Vec<PolicyViolation> {
    let s = STATE.lock().unwrap();
    let start = s.violations.len().saturating_sub(n);
    s.violations[start..].to_vec()
}

pub fn is_dangerous_pattern(arg: &str) -> bool {
    let lower = arg.to_lowercase();
    DANGEROUS_PATTERNS.iter().any(|p| lower.contains(p))
}

pub fn policy_checks()    -> u64 { POLICY_CHECKS.load(Ordering::Relaxed) }
pub fn policy_blocked()   -> u64 { POLICY_BLOCKED.load(Ordering::Relaxed) }
pub fn policy_violations() -> u64 { POLICY_VIOLATIONS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_pattern_always_blocked() {
        set_policy(SecurityPolicy::Relaxed);
        let result = check("shell", "rm -rf /");
        assert!(result.is_err());
    }

    #[test]
    fn safe_command_passes_standard() {
        set_policy(SecurityPolicy::Standard);
        let result = check("notes", "write meeting notes");
        assert!(result.is_ok());
    }

    #[test]
    fn exec_blocked_in_strict() {
        set_policy(SecurityPolicy::Strict);
        let result = check("terminal_exec", "ls -la");
        assert!(result.is_err());
    }

    #[test]
    fn protected_path_blocked_standard() {
        set_policy(SecurityPolicy::Standard);
        let result = check("file_read", "C:\\Windows\\System32\\important.dll");
        assert!(result.is_err());
    }

    #[test]
    fn is_dangerous_pattern_detection() {
        assert!(is_dangerous_pattern("rm -rf /home"));
        assert!(!is_dangerous_pattern("echo hello"));
    }
}
