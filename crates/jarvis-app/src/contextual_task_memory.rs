//! Contextual task memory for the autonomous runtime.
//!
//! Tracks task lifecycle across the full autonomous execution pipeline:
//!   Active → Completed | Failed | Recovering | Abandoned
//!
//! Provides recovery history, tool outcomes, and planner context
//! so the runtime can make informed decisions on subsequent runs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;

pub static TASKS_CREATED:   AtomicU64 = AtomicU64::new(0);
pub static TASKS_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static TASKS_FAILED:    AtomicU64 = AtomicU64::new(0);
pub static TASKS_RECOVERED: AtomicU64 = AtomicU64::new(0);

// ── Task status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum TaskMemoryStatus {
    Active,
    Completed,
    Failed,
    Recovering,
    Abandoned,
}

// ── Tool outcome record ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolOutcomeRecord {
    pub tool_id:    String,
    pub success:    bool,
    pub latency_ms: u64,
    pub ts_ms:      u64,
}

// ── Task memory entry ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskMemoryEntry {
    pub id:              u64,
    pub goal:            String,
    pub status:          TaskMemoryStatus,
    pub created_ms:      u64,
    pub completed_ms:    Option<u64>,
    pub recovery_count:  u8,
    pub tool_outcomes:   Vec<ToolOutcomeRecord>,
    pub failure_reason:  Option<String>,
}

impl TaskMemoryEntry {
    pub fn duration_ms(&self) -> Option<u64> {
        self.completed_ms.map(|c| c.saturating_sub(self.created_ms))
    }
}

// ── Memory store ──────────────────────────────────────────────────────────────

static STORE: Lazy<Mutex<ContextualTaskMemoryStore>> =
    Lazy::new(|| Mutex::new(ContextualTaskMemoryStore::new()));

static TASK_ID_SEQ: AtomicU64 = AtomicU64::new(1);

struct ContextualTaskMemoryStore {
    entries: HashMap<u64, TaskMemoryEntry>,
}

impl ContextualTaskMemoryStore {
    fn new() -> Self {
        Self { entries: HashMap::new() }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Create a new active task.  Returns its ID.
pub fn push_task(goal: impl Into<String>) -> u64 {
    let id = TASK_ID_SEQ.fetch_add(1, Ordering::Relaxed);
    TASKS_CREATED.fetch_add(1, Ordering::Relaxed);
    let entry = TaskMemoryEntry {
        id,
        goal: goal.into(),
        status: TaskMemoryStatus::Active,
        created_ms: now_ms(),
        completed_ms: None,
        recovery_count: 0,
        tool_outcomes: Vec::new(),
        failure_reason: None,
    };
    STORE.lock().entries.insert(id, entry);
    id
}

/// Mark a task as completed.
pub fn complete_task(id: u64) {
    let mut store = STORE.lock();
    if let Some(entry) = store.entries.get_mut(&id) {
        entry.status = TaskMemoryStatus::Completed;
        entry.completed_ms = Some(now_ms());
        TASKS_COMPLETED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Mark a task as failed with a reason.
pub fn fail_task(id: u64, reason: impl Into<String>) {
    let mut store = STORE.lock();
    if let Some(entry) = store.entries.get_mut(&id) {
        entry.status = TaskMemoryStatus::Failed;
        entry.completed_ms = Some(now_ms());
        entry.failure_reason = Some(reason.into());
        TASKS_FAILED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Increment recovery count and set status to Recovering.
pub fn mark_recovering(id: u64) {
    let mut store = STORE.lock();
    if let Some(entry) = store.entries.get_mut(&id) {
        entry.status = TaskMemoryStatus::Recovering;
        entry.recovery_count = entry.recovery_count.saturating_add(1);
        TASKS_RECOVERED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Record a tool outcome for a task.
pub fn record_tool_outcome(id: u64, tool_id: impl Into<String>, success: bool, latency_ms: u64) {
    let mut store = STORE.lock();
    if let Some(entry) = store.entries.get_mut(&id) {
        entry.tool_outcomes.push(ToolOutcomeRecord {
            tool_id: tool_id.into(),
            success,
            latency_ms,
            ts_ms: now_ms(),
        });
    }
}

/// Get a snapshot of a task entry.
pub fn get_task(id: u64) -> Option<TaskMemoryEntry> {
    STORE.lock().entries.get(&id).cloned()
}

/// Count of entries by status.
pub fn count_by_status(status: &TaskMemoryStatus) -> usize {
    STORE.lock().entries.values().filter(|e| &e.status == status).count()
}

/// Write current store to `contextual_memory.json`.
pub fn write_snapshot() {
    let entries: Vec<TaskMemoryEntry> = {
        let store = STORE.lock();
        store.entries.values().cloned().collect()
    };
    if let Ok(json) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write("contextual_memory.json", json);
    }
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

    #[test]
    fn push_and_complete_task() {
        let id = push_task("open IDE");
        let entry = get_task(id).unwrap();
        assert_eq!(entry.status, TaskMemoryStatus::Active);
        complete_task(id);
        let entry = get_task(id).unwrap();
        assert_eq!(entry.status, TaskMemoryStatus::Completed);
        assert!(entry.completed_ms.is_some());
    }

    #[test]
    fn fail_task_stores_reason() {
        let id = push_task("start docker");
        fail_task(id, "port 8080 unavailable");
        let entry = get_task(id).unwrap();
        assert_eq!(entry.status, TaskMemoryStatus::Failed);
        assert!(entry.failure_reason.as_deref().unwrap().contains("8080"));
    }

    #[test]
    fn recovering_increments_count() {
        let id = push_task("retry docker");
        mark_recovering(id);
        let entry = get_task(id).unwrap();
        assert_eq!(entry.recovery_count, 1);
        assert_eq!(entry.status, TaskMemoryStatus::Recovering);
    }

    #[test]
    fn tool_outcome_recorded() {
        let id = push_task("test");
        record_tool_outcome(id, "app.open", true, 42);
        let entry = get_task(id).unwrap();
        assert_eq!(entry.tool_outcomes.len(), 1);
        assert!(entry.tool_outcomes[0].success);
    }

    #[test]
    fn tasks_created_counter_increments() {
        let before = TASKS_CREATED.load(Ordering::Relaxed);
        push_task("counter test");
        assert!(TASKS_CREATED.load(Ordering::Relaxed) > before);
    }
}
