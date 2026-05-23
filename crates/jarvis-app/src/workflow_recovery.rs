//! Workflow recovery — restores interrupted workflows, resumes failed tasks,
//! and recovers context after crashes or safe-mode transitions.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static WORKFLOWS_SAVED:     AtomicU64 = AtomicU64::new(0);
pub static WORKFLOWS_RECOVERED: AtomicU64 = AtomicU64::new(0);
pub static RECOVERY_ATTEMPTS:   AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WorkflowState {
    Running,
    Interrupted,
    Paused,
    Completed,
    Failed,
}

impl WorkflowState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running     => "Running",
            Self::Interrupted => "Interrupted",
            Self::Paused      => "Paused",
            Self::Completed   => "Completed",
            Self::Failed      => "Failed",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowSession {
    pub id:           u64,
    pub name:         String,
    pub state:        WorkflowState,
    pub current_step: usize,
    pub total_steps:  usize,
    pub context:      String,
    pub tools_used:   Vec<String>,
    pub started_at:   u64,
    pub updated_at:   u64,
}

const MAX_SESSIONS: usize = 50;

struct RecoveryState {
    sessions: Vec<WorkflowSession>,
    next_id:  u64,
}

impl RecoveryState {
    fn new() -> Self { Self { sessions: Vec::new(), next_id: 1 } }
}

static STATE: Lazy<Mutex<RecoveryState>> = Lazy::new(|| Mutex::new(RecoveryState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn save_workflow(name: &str, step: usize, total: usize, context: &str, tools: Vec<String>) -> u64 {
    let mut s = STATE.lock().unwrap();
    // Update existing if found
    if let Some(existing) = s.sessions.iter_mut().find(|w| w.name == name) {
        existing.current_step = step;
        existing.total_steps  = total;
        existing.context      = context.to_string();
        existing.tools_used   = tools;
        existing.updated_at   = ts_now();
        existing.state        = WorkflowState::Running;
        WORKFLOWS_SAVED.fetch_add(1, Ordering::Relaxed);
        return existing.id;
    }
    // Create new
    let id = s.next_id;
    s.next_id += 1;
    if s.sessions.len() >= MAX_SESSIONS { s.sessions.remove(0); }
    let now = ts_now();
    s.sessions.push(WorkflowSession {
        id, name: name.to_string(), state: WorkflowState::Running,
        current_step: step, total_steps: total,
        context: context.to_string(), tools_used: tools,
        started_at: now, updated_at: now,
    });
    WORKFLOWS_SAVED.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn mark_interrupted(workflow_name: &str) {
    let mut s = STATE.lock().unwrap();
    if let Some(w) = s.sessions.iter_mut().find(|w| w.name == workflow_name) {
        w.state = WorkflowState::Interrupted;
        w.updated_at = ts_now();
    }
}

pub fn mark_completed(workflow_name: &str) {
    let mut s = STATE.lock().unwrap();
    if let Some(w) = s.sessions.iter_mut().find(|w| w.name == workflow_name) {
        w.state = WorkflowState::Completed;
        w.updated_at = ts_now();
    }
}

pub fn interrupted_workflows() -> Vec<WorkflowSession> {
    let s = STATE.lock().unwrap();
    s.sessions.iter()
        .filter(|w| matches!(w.state, WorkflowState::Interrupted | WorkflowState::Failed))
        .cloned().collect()
}

pub fn recover_workflow(workflow_name: &str) -> Option<WorkflowSession> {
    RECOVERY_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    let mut s = STATE.lock().unwrap();
    if let Some(w) = s.sessions.iter_mut().find(|w| w.name == workflow_name) {
        if matches!(w.state, WorkflowState::Interrupted | WorkflowState::Failed) {
            w.state = WorkflowState::Running;
            w.updated_at = ts_now();
            WORKFLOWS_RECOVERED.fetch_add(1, Ordering::Relaxed);
            crate::production_logging::info("workflow_recovery",
                &format!("recovered workflow '{}' at step {}/{}", w.name, w.current_step, w.total_steps));
            return Some(w.clone());
        }
    }
    None
}

pub fn all_workflows() -> Vec<WorkflowSession> {
    STATE.lock().unwrap().sessions.clone()
}

pub fn recover_all_interrupted() -> usize {
    let names: Vec<String> = {
        let s = STATE.lock().unwrap();
        s.sessions.iter()
            .filter(|w| matches!(w.state, WorkflowState::Interrupted | WorkflowState::Failed))
            .map(|w| w.name.clone())
            .collect()
    };
    let count = names.len();
    for name in names { recover_workflow(&name); }
    count
}

#[derive(Debug, serde::Serialize)]
pub struct RecoverySnapshot {
    pub workflows_saved:     u64,
    pub workflows_recovered: u64,
    pub recovery_attempts:   u64,
    pub active_workflows:    usize,
    pub interrupted_count:   usize,
}

pub fn snapshot() -> RecoverySnapshot {
    let s = STATE.lock().unwrap();
    let interrupted = s.sessions.iter()
        .filter(|w| matches!(w.state, WorkflowState::Interrupted | WorkflowState::Failed))
        .count();
    let active = s.sessions.iter()
        .filter(|w| matches!(w.state, WorkflowState::Running | WorkflowState::Paused))
        .count();
    RecoverySnapshot {
        workflows_saved:     WORKFLOWS_SAVED.load(Ordering::Relaxed),
        workflows_recovered: WORKFLOWS_RECOVERED.load(Ordering::Relaxed),
        recovery_attempts:   RECOVERY_ATTEMPTS.load(Ordering::Relaxed),
        active_workflows:    active,
        interrupted_count:   interrupted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_retrieve() {
        let id = save_workflow("test_wf", 2, 5, "doing step 2", vec!["file_read".to_string()]);
        assert!(id > 0);
        let all = all_workflows();
        assert!(all.iter().any(|w| w.id == id));
    }

    #[test]
    fn interrupt_and_recover() {
        save_workflow("interrupted_wf", 3, 7, "mid-task", vec![]);
        mark_interrupted("interrupted_wf");
        let wf = recover_workflow("interrupted_wf");
        assert!(wf.is_some());
        assert!(matches!(wf.unwrap().state, WorkflowState::Running));
    }

    #[test]
    fn recover_nonexistent_returns_none() {
        let result = recover_workflow("does_not_exist_xyz");
        assert!(result.is_none());
    }

    #[test]
    fn mark_completed_removes_from_interrupted() {
        save_workflow("complete_wf", 5, 5, "done", vec![]);
        mark_completed("complete_wf");
        let int = interrupted_workflows();
        assert!(!int.iter().any(|w| w.name == "complete_wf"));
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        let _ = s.workflows_saved;
    }
}
