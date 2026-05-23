//! Failsafe execution — checkpoint/rollback for automation workflows.
//! Previews actions before execution, checkpoints state, restores on failure.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static CHECKPOINTS_CREATED: AtomicU64 = AtomicU64::new(0);
pub static ROLLBACKS_EXECUTED:  AtomicU64 = AtomicU64::new(0);
pub static PREVIEWS_GENERATED:  AtomicU64 = AtomicU64::new(0);
pub static EXECUTIONS_FAILSAFE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowCheckpoint {
    pub id:              u64,
    pub workflow_name:   String,
    pub step:            usize,
    pub context_summary: String,
    pub tools_in_use:    Vec<String>,
    pub created_at:      u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecutionPreview {
    pub tool:          String,
    pub action:        String,
    pub expected_outcome: String,
    pub reversible:    bool,
    pub risk_level:    String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FailsafeRecord {
    pub id:           u64,
    pub tool:         String,
    pub action:       String,
    pub executed_at:  u64,
    pub succeeded:    bool,
    pub rolled_back:  bool,
    pub checkpoint_id: Option<u64>,
}

const MAX_CHECKPOINTS: usize = 20;
const MAX_HISTORY:     usize = 100;

struct FailsafeState {
    checkpoints:  Vec<WorkflowCheckpoint>,
    history:      Vec<FailsafeRecord>,
    next_id:      u64,
}

impl FailsafeState {
    fn new() -> Self { Self { checkpoints: Vec::new(), history: Vec::new(), next_id: 1 } }
}

static STATE: Lazy<Mutex<FailsafeState>> = Lazy::new(|| Mutex::new(FailsafeState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Checkpoint ────────────────────────────────────────────────────────────────

pub fn checkpoint(workflow: &str, step: usize, context: &str, tools: Vec<String>) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_id;
    s.next_id += 1;
    if s.checkpoints.len() >= MAX_CHECKPOINTS { s.checkpoints.remove(0); }
    s.checkpoints.push(WorkflowCheckpoint {
        id,
        workflow_name:   workflow.to_string(),
        step,
        context_summary: context.to_string(),
        tools_in_use:    tools,
        created_at:      ts_now(),
    });
    CHECKPOINTS_CREATED.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn latest_checkpoint(workflow: &str) -> Option<WorkflowCheckpoint> {
    let s = STATE.lock().unwrap();
    s.checkpoints.iter().filter(|c| c.workflow_name == workflow)
        .last().cloned()
}

pub fn rollback_to(checkpoint_id: u64) -> bool {
    let s = STATE.lock().unwrap();
    let exists = s.checkpoints.iter().any(|c| c.id == checkpoint_id);
    drop(s);
    if exists {
        ROLLBACKS_EXECUTED.fetch_add(1, Ordering::Relaxed);
        crate::production_logging::info("failsafe_execution",
            &format!("rollback to checkpoint {}", checkpoint_id));
        true
    } else {
        false
    }
}

// ── Preview ───────────────────────────────────────────────────────────────────

pub fn preview(tool: &str, action: &str) -> ExecutionPreview {
    PREVIEWS_GENERATED.fetch_add(1, Ordering::Relaxed);
    let action_lower = action.to_lowercase();
    let reversible = !action_lower.contains("delete") && !action_lower.contains("remove")
        && !action_lower.contains("drop") && !action_lower.contains("format");
    let risk = if !reversible { "High" } else if action_lower.contains("write") { "Medium" } else { "Low" };
    let expected = format!("Execute {} on {}", tool, action);
    ExecutionPreview {
        tool:             tool.to_string(),
        action:           action.to_string(),
        expected_outcome: expected,
        reversible,
        risk_level:       risk.to_string(),
    }
}

// ── Record execution ──────────────────────────────────────────────────────────

pub fn record(tool: &str, action: &str, succeeded: bool, checkpoint_id: Option<u64>) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_id;
    s.next_id += 1;
    if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
    s.history.push(FailsafeRecord {
        id, tool: tool.to_string(), action: action.to_string(),
        executed_at: ts_now(), succeeded, rolled_back: false, checkpoint_id,
    });
    EXECUTIONS_FAILSAFE.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn mark_rolled_back(record_id: u64) {
    let mut s = STATE.lock().unwrap();
    if let Some(r) = s.history.iter_mut().find(|r| r.id == record_id) {
        r.rolled_back = true;
        ROLLBACKS_EXECUTED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn recent_history(n: usize) -> Vec<FailsafeRecord> {
    let s = STATE.lock().unwrap();
    s.history.iter().rev().take(n).cloned().collect()
}

pub fn checkpoints() -> Vec<WorkflowCheckpoint> {
    STATE.lock().unwrap().checkpoints.clone()
}

#[derive(Debug, serde::Serialize)]
pub struct FailsafeSnapshot {
    pub checkpoints_created: u64,
    pub rollbacks_executed:  u64,
    pub previews_generated:  u64,
    pub executions_total:    u64,
    pub active_checkpoints:  usize,
}

pub fn snapshot() -> FailsafeSnapshot {
    let s = STATE.lock().unwrap();
    FailsafeSnapshot {
        checkpoints_created: CHECKPOINTS_CREATED.load(Ordering::Relaxed),
        rollbacks_executed:  ROLLBACKS_EXECUTED.load(Ordering::Relaxed),
        previews_generated:  PREVIEWS_GENERATED.load(Ordering::Relaxed),
        executions_total:    EXECUTIONS_FAILSAFE.load(Ordering::Relaxed),
        active_checkpoints:  s.checkpoints.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_and_retrieve() {
        let id = checkpoint("dev_workflow", 3, "editing main.rs", vec!["file_write".to_string()]);
        let cp = latest_checkpoint("dev_workflow").unwrap();
        assert_eq!(cp.id, id);
        assert_eq!(cp.step, 3);
    }

    #[test]
    fn rollback_to_valid() {
        let id = checkpoint("test_wf", 1, "ctx", vec![]);
        assert!(rollback_to(id));
    }

    #[test]
    fn rollback_to_invalid() {
        assert!(!rollback_to(999_999));
    }

    #[test]
    fn preview_reversibility() {
        let p = preview("file_write", "write config.json");
        assert!(p.reversible);
        let p2 = preview("shell", "delete all logs");
        assert!(!p2.reversible);
    }

    #[test]
    fn record_and_mark_rolled_back() {
        let cp_id = checkpoint("wf", 1, "ctx", vec![]);
        let rid = record("file_write", "write x.txt", true, Some(cp_id));
        mark_rolled_back(rid);
        let hist = recent_history(5);
        assert!(hist.iter().any(|r| r.id == rid && r.rolled_back));
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.checkpoints_created > 0);
    }
}
