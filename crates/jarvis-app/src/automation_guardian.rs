//! Automation guardian — safe execution UX for all Jarvis-initiated actions.
//! Every action goes through explain → confirm → execute → record → rollback-capable.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static ACTIONS_PROPOSED:  AtomicU64 = AtomicU64::new(0);
pub static ACTIONS_CONFIRMED: AtomicU64 = AtomicU64::new(0);
pub static ACTIONS_BLOCKED:   AtomicU64 = AtomicU64::new(0);
pub static ACTIONS_ROLLED_BACK: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingAction {
    pub id:          u64,
    pub tool:        String,
    pub description: String,
    pub preview:     String,
    pub risk:        RiskLevel,
    pub proposed_at: u64,
    pub confirmed:   bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutionRecord {
    pub id:          u64,
    pub tool:        String,
    pub description: String,
    pub risk:        RiskLevel,
    pub executed_at: u64,
    pub outcome:     String,
    pub rolled_back: bool,
}

struct GuardianState {
    pending:       Vec<PendingAction>,
    history:       Vec<ExecutionRecord>,
    next_id:       u64,
}

impl GuardianState {
    fn new() -> Self {
        Self { pending: Vec::new(), history: Vec::new(), next_id: 1 }
    }
}

static STATE: Lazy<Mutex<GuardianState>> = Lazy::new(|| Mutex::new(GuardianState::new()));

const HISTORY_CAP: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn classify_risk(tool: &str, description: &str) -> RiskLevel {
    let combined = format!("{} {}", tool, description).to_lowercase();
    if combined.contains("delete") || combined.contains("remove") || combined.contains("drop")
        || combined.contains("format") || combined.contains("shutdown")
    {
        RiskLevel::Critical
    } else if combined.contains("write") || combined.contains("execute") || combined.contains("run")
        || combined.contains("install") || combined.contains("modify")
    {
        RiskLevel::High
    } else if combined.contains("read") || combined.contains("open") || combined.contains("search") {
        RiskLevel::Low
    } else {
        RiskLevel::Medium
    }
}

/// Propose an action for user confirmation. Returns the action ID.
pub fn propose(tool: &str, description: &str, preview: &str) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_id;
    s.next_id += 1;
    let risk = classify_risk(tool, description);
    s.pending.push(PendingAction {
        id,
        tool:        tool.to_string(),
        description: description.to_string(),
        preview:     preview.to_string(),
        risk,
        proposed_at: ts_now(),
        confirmed:   false,
    });
    ACTIONS_PROPOSED.fetch_add(1, Ordering::Relaxed);
    id
}

/// User confirmed the action.
pub fn confirm(action_id: u64) -> bool {
    let mut s = STATE.lock().unwrap();
    if let Some(a) = s.pending.iter_mut().find(|a| a.id == action_id) {
        a.confirmed = true;
        ACTIONS_CONFIRMED.fetch_add(1, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// Block (reject) a pending action.
pub fn block(action_id: u64) -> bool {
    let mut s = STATE.lock().unwrap();
    let before = s.pending.len();
    s.pending.retain(|a| a.id != action_id);
    let blocked = s.pending.len() < before;
    if blocked {
        ACTIONS_BLOCKED.fetch_add(1, Ordering::Relaxed);
    }
    blocked
}

/// Record execution of a confirmed action. Drains it from pending.
pub fn record_execution(action_id: u64, outcome: &str) {
    let mut s = STATE.lock().unwrap();
    let action = s.pending.iter().find(|a| a.id == action_id).cloned();
    s.pending.retain(|a| a.id != action_id);
    if let Some(a) = action {
        if s.history.len() >= HISTORY_CAP {
            s.history.remove(0);
        }
        s.history.push(ExecutionRecord {
            id:          a.id,
            tool:        a.tool.clone(),
            description: a.description.clone(),
            risk:        a.risk.clone(),
            executed_at: ts_now(),
            outcome:     outcome.to_string(),
            rolled_back: false,
        });
    }
}

/// Mark a previously executed action as rolled back.
pub fn rollback(action_id: u64) -> bool {
    let mut s = STATE.lock().unwrap();
    if let Some(r) = s.history.iter_mut().find(|r| r.id == action_id) {
        r.rolled_back = true;
        ACTIONS_ROLLED_BACK.fetch_add(1, Ordering::Relaxed);
        true
    } else {
        false
    }
}

pub fn pending_actions()    -> Vec<PendingAction>   { STATE.lock().unwrap().pending.clone() }
pub fn execution_history()  -> Vec<ExecutionRecord>  { STATE.lock().unwrap().history.clone() }
pub fn is_confirmed(id: u64) -> bool {
    STATE.lock().unwrap().pending.iter().any(|a| a.id == id && a.confirmed)
}

#[derive(Debug, serde::Serialize)]
pub struct GuardianSnapshot {
    pub pending_count:   usize,
    pub history_count:   usize,
    pub proposed_total:  u64,
    pub confirmed_total: u64,
    pub blocked_total:   u64,
    pub rolled_back:     u64,
}

pub fn snapshot() -> GuardianSnapshot {
    let s = STATE.lock().unwrap();
    GuardianSnapshot {
        pending_count:   s.pending.len(),
        history_count:   s.history.len(),
        proposed_total:  ACTIONS_PROPOSED.load(Ordering::Relaxed),
        confirmed_total: ACTIONS_CONFIRMED.load(Ordering::Relaxed),
        blocked_total:   ACTIONS_BLOCKED.load(Ordering::Relaxed),
        rolled_back:     ACTIONS_ROLLED_BACK.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propose_and_confirm() {
        let id = propose("file_write", "write config.json", "{ ... }");
        assert!(id > 0);
        let ok = confirm(id);
        assert!(ok);
        assert!(is_confirmed(id));
    }

    #[test]
    fn block_removes_from_pending() {
        let id = propose("terminal_exec", "run script.ps1", "powershell script.ps1");
        let ok = block(id);
        assert!(ok);
        assert!(!pending_actions().iter().any(|a| a.id == id));
    }

    #[test]
    fn record_execution_moves_to_history() {
        let id = propose("file_read", "read notes.txt", "cat notes.txt");
        confirm(id);
        record_execution(id, "success");
        let hist = execution_history();
        assert!(hist.iter().any(|r| r.id == id && r.outcome == "success"));
    }

    #[test]
    fn rollback_marks_record() {
        let id = propose("file_write", "write temp.txt", "...");
        confirm(id);
        record_execution(id, "ok");
        let ok = rollback(id);
        assert!(ok);
        let hist = execution_history();
        assert!(hist.iter().any(|r| r.id == id && r.rolled_back));
    }

    #[test]
    fn critical_risk_detection() {
        let id = propose("tool", "delete all files in C:\\temp", "rm -rf");
        let pending = pending_actions();
        let action = pending.iter().find(|a| a.id == id).unwrap();
        assert!(matches!(action.risk, RiskLevel::Critical));
        block(id);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        let _ = s.pending_count;
    }
}
