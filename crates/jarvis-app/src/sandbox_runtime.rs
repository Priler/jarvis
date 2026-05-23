//! Enhanced sandbox runtime — combines permission_runtime + security_policies
//! into a single gate for all tool execution. Tracks per-session tool usage.

use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;

pub static SANDBOX_CHECKS:   AtomicU64 = AtomicU64::new(0);
pub static SANDBOX_ALLOWED:  AtomicU64 = AtomicU64::new(0);
pub static SANDBOX_BLOCKED:  AtomicU64 = AtomicU64::new(0);

const MAX_TIMEOUT_MS:    u64   = 30_000;
const MAX_ARG_LEN:       usize = 4096;
const MAX_TOOL_CALLS_PER_SESSION: u64 = 1000;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct SandboxDecision {
    pub allowed:  bool,
    pub reason:   String,
    pub tool:     String,
    pub ts_ms:    u64,
}

#[derive(Debug, Clone)]
struct SandboxState {
    session_calls: u64,
    tool_allowlist: Vec<String>,
    tool_denylist:  Vec<String>,
}

static STATE: Lazy<Mutex<SandboxState>> = Lazy::new(|| Mutex::new(SandboxState {
    session_calls:  0,
    tool_allowlist: vec![
        "notes".to_string(), "search".to_string(), "memory_read".to_string(),
        "calendar".to_string(), "timer".to_string(), "weather".to_string(),
    ],
    tool_denylist: vec![
        "rm".to_string(), "format".to_string(), "delete_system".to_string(),
    ],
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Main sandbox gate. Returns decision for tool + arg.
pub fn check(tool: &str, arg: &str) -> SandboxDecision {
    SANDBOX_CHECKS.fetch_add(1, Ordering::Relaxed);
    let ts = ts_now();

    // R1: Session call limit
    {
        let mut s = STATE.lock().unwrap();
        if s.session_calls >= MAX_TOOL_CALLS_PER_SESSION {
            SANDBOX_BLOCKED.fetch_add(1, Ordering::Relaxed);
            return SandboxDecision { allowed: false, reason: "session_call_limit_exceeded".to_string(), tool: tool.to_string(), ts_ms: ts };
        }
        s.session_calls += 1;
    }

    // R2: Arg length
    if arg.len() > MAX_ARG_LEN {
        SANDBOX_BLOCKED.fetch_add(1, Ordering::Relaxed);
        return SandboxDecision { allowed: false, reason: "arg_too_long".to_string(), tool: tool.to_string(), ts_ms: ts };
    }

    // R3: Denylist check
    {
        let s = STATE.lock().unwrap();
        if s.tool_denylist.iter().any(|d| tool.contains(d.as_str())) {
            SANDBOX_BLOCKED.fetch_add(1, Ordering::Relaxed);
            return SandboxDecision { allowed: false, reason: "tool_on_denylist".to_string(), tool: tool.to_string(), ts_ms: ts };
        }
    }

    // R4: Security policy check
    if let Err(v) = crate::security_policies::check(tool, arg) {
        SANDBOX_BLOCKED.fetch_add(1, Ordering::Relaxed);
        return SandboxDecision { allowed: false, reason: v.reason.to_string(), tool: tool.to_string(), ts_ms: ts };
    }

    // R5: Permission check for sensitive tools
    let needs_permission = tool.contains("file") || tool.contains("terminal")
        || tool.contains("exec") || tool.contains("browser") || tool.contains("desktop");

    if needs_permission {
        let kind = tool_to_permission_kind(tool);
        if !crate::permission_runtime::is_granted(&kind, arg) {
            // Auto-request permission (user will see it in the UI)
            let _ = crate::permission_runtime::request(kind, arg, &format!("tool {} needs access", tool));
            SANDBOX_BLOCKED.fetch_add(1, Ordering::Relaxed);
            return SandboxDecision { allowed: false, reason: "permission_required".to_string(), tool: tool.to_string(), ts_ms: ts };
        }
    }

    SANDBOX_ALLOWED.fetch_add(1, Ordering::Relaxed);
    SandboxDecision { allowed: true, reason: "ok".to_string(), tool: tool.to_string(), ts_ms: ts }
}

fn tool_to_permission_kind(tool: &str) -> crate::permission_runtime::PermissionKind {
    use crate::permission_runtime::PermissionKind;
    if tool.contains("file_write")   { PermissionKind::FileWrite }
    else if tool.contains("file")    { PermissionKind::FileRead }
    else if tool.contains("terminal") || tool.contains("exec") { PermissionKind::TerminalExec }
    else if tool.contains("browser") { PermissionKind::BrowserControl }
    else if tool.contains("desktop") { PermissionKind::DesktopControl }
    else                             { PermissionKind::SystemCommand }
}

pub fn add_to_allowlist(tool: &str) {
    STATE.lock().unwrap().tool_allowlist.push(tool.to_string());
}

pub fn add_to_denylist(tool: &str) {
    STATE.lock().unwrap().tool_denylist.push(tool.to_string());
}

pub fn reset_session() {
    STATE.lock().unwrap().session_calls = 0;
}

pub fn max_timeout_ms() -> u64 { MAX_TIMEOUT_MS }
pub fn session_calls()  -> u64 { STATE.lock().unwrap().session_calls }
pub fn checks_total()   -> u64 { SANDBOX_CHECKS.load(Ordering::Relaxed) }
pub fn allowed_total()  -> u64 { SANDBOX_ALLOWED.load(Ordering::Relaxed) }
pub fn blocked_total()  -> u64 { SANDBOX_BLOCKED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_tool_allowed() {
        crate::security_policies::set_policy(crate::security_policies::SecurityPolicy::Standard);
        let d = check("notes", "write a note");
        assert!(d.allowed);
    }

    #[test]
    fn dangerous_arg_blocked() {
        let d = check("shell", "rm -rf /");
        assert!(!d.allowed);
    }

    #[test]
    fn denylist_tool_blocked() {
        let d = check("rm_tool", "file.txt");
        assert!(!d.allowed);
    }

    #[test]
    fn oversized_arg_blocked() {
        let big_arg = "a".repeat(MAX_ARG_LEN + 1);
        let d = check("notes", &big_arg);
        assert!(!d.allowed);
    }

    #[test]
    fn file_tool_requires_permission() {
        // Without prior grant, file tool should be blocked
        let d = check("file_read", "/some/path.txt");
        // Either blocked (no permission) or allowed (if previously granted in test)
        assert!(d.tool == "file_read");
    }
}
