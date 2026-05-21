//! Goal runtime — tracks active, completed, and abandoned goals.
//!
//! Goals are high-level intents (e.g., "open IDE", "close browser").
//! The goal store is a singleton; goals are created by the planner and
//! evaluated each cognition tick.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static GOALS_CREATED:   AtomicU64 = AtomicU64::new(0);
pub static GOALS_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static GOALS_ABANDONED: AtomicU64 = AtomicU64::new(0);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

// ── Goal kinds ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GoalKind {
    OpenApplication  { process: String },
    CloseApplication { process: String },
    NavigateTo       { target: String },
    ExecuteTool      { tool_id: String, arg: Option<String> },
    MonitorCondition { description: String },
    Generic          { description: String },
}

impl GoalKind {
    pub fn description(&self) -> String {
        match self {
            GoalKind::OpenApplication  { process }  => format!("open {}", process),
            GoalKind::CloseApplication { process }  => format!("close {}", process),
            GoalKind::NavigateTo       { target }   => format!("navigate to {}", target),
            GoalKind::ExecuteTool      { tool_id, .. } => format!("execute {}", tool_id),
            GoalKind::MonitorCondition { description } => format!("monitor: {}", description),
            GoalKind::Generic          { description } => description.clone(),
        }
    }
}

// ── Goal status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GoalStatus {
    Pending,
    Active,
    Completed,
    Abandoned { reason: String },
    Blocked   { reason: String },
}

impl GoalStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, GoalStatus::Completed | GoalStatus::Abandoned { .. })
    }
}

// ── Goal ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    pub id:         u64,
    pub kind:       GoalKind,
    pub status:     GoalStatus,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub attempts:   u32,
}

impl Goal {
    pub fn new(kind: GoalKind) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        GOALS_CREATED.fetch_add(1, Ordering::Relaxed);
        let now = ts_now();
        Self {
            id,
            kind,
            status: GoalStatus::Pending,
            created_ms: now,
            updated_ms: now,
            attempts: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, GoalStatus::Active | GoalStatus::Pending)
    }

    fn touch(&mut self) {
        self.updated_ms = ts_now();
    }
}

// ── Goal store ────────────────────────────────────────────────────────────────

static GOALS: Lazy<Mutex<Vec<Goal>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub struct GoalRuntime;

impl GoalRuntime {
    pub fn add(kind: GoalKind) -> u64 {
        let goal = Goal::new(kind);
        let id = goal.id;
        if let Ok(mut guard) = GOALS.lock() {
            guard.push(goal);
        }
        id
    }

    pub fn activate(id: u64) {
        if let Ok(mut guard) = GOALS.lock() {
            if let Some(g) = guard.iter_mut().find(|g| g.id == id) {
                g.status = GoalStatus::Active;
                g.touch();
            }
        }
    }

    pub fn complete(id: u64) {
        if let Ok(mut guard) = GOALS.lock() {
            if let Some(g) = guard.iter_mut().find(|g| g.id == id) {
                g.status = GoalStatus::Completed;
                g.touch();
                GOALS_COMPLETED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn abandon(id: u64, reason: impl Into<String>) {
        if let Ok(mut guard) = GOALS.lock() {
            if let Some(g) = guard.iter_mut().find(|g| g.id == id) {
                g.status = GoalStatus::Abandoned { reason: reason.into() };
                g.touch();
                GOALS_ABANDONED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn increment_attempts(id: u64) {
        if let Ok(mut guard) = GOALS.lock() {
            if let Some(g) = guard.iter_mut().find(|g| g.id == id) {
                g.attempts += 1;
                g.touch();
            }
        }
    }

    pub fn active_goals() -> Vec<Goal> {
        GOALS.lock().map(|g| g.iter().filter(|g| g.is_active()).cloned().collect())
            .unwrap_or_default()
    }

    pub fn all_goals() -> Vec<Goal> {
        GOALS.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn get(id: u64) -> Option<Goal> {
        GOALS.lock().ok().and_then(|g| g.iter().find(|g| g.id == id).cloned())
    }

    pub fn has_active_goals() -> bool {
        GOALS.lock().map(|g| g.iter().any(|g| g.is_active())).unwrap_or(false)
    }

    pub fn clear_terminal() {
        if let Ok(mut guard) = GOALS.lock() {
            guard.retain(|g| !g.status.is_terminal());
        }
    }

    pub fn clear_all() {
        if let Ok(mut guard) = GOALS.lock() {
            guard.clear();
        }
    }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup() {
        GoalRuntime::clear_all();
    }

    #[test]
    fn create_and_retrieve_goal() {
        cleanup();
        let id = GoalRuntime::add(GoalKind::Generic { description: "test goal".into() });
        let goal = GoalRuntime::get(id).unwrap();
        assert_eq!(goal.id, id);
        assert!(goal.is_active());
    }

    #[test]
    fn complete_goal_marks_terminal() {
        cleanup();
        let id = GoalRuntime::add(GoalKind::OpenApplication { process: "vscode".into() });
        GoalRuntime::complete(id);
        let goal = GoalRuntime::get(id).unwrap();
        assert!(goal.status.is_terminal());
    }

    #[test]
    fn abandon_goal_with_reason() {
        cleanup();
        let id = GoalRuntime::add(GoalKind::Generic { description: "old goal".into() });
        GoalRuntime::abandon(id, "timed out");
        let goal = GoalRuntime::get(id).unwrap();
        assert!(matches!(goal.status, GoalStatus::Abandoned { .. }));
    }

    #[test]
    fn active_goals_excludes_terminal() {
        cleanup();
        let id1 = GoalRuntime::add(GoalKind::Generic { description: "a".into() });
        let id2 = GoalRuntime::add(GoalKind::Generic { description: "b".into() });
        GoalRuntime::complete(id1);
        let active = GoalRuntime::active_goals();
        assert!(active.iter().any(|g| g.id == id2));
        assert!(!active.iter().any(|g| g.id == id1));
    }

    #[test]
    fn goals_created_counter_increments() {
        let before = GOALS_CREATED.load(Ordering::Relaxed);
        GoalRuntime::add(GoalKind::Generic { description: "counter test".into() });
        assert!(GOALS_CREATED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn goal_kind_description() {
        let kind = GoalKind::OpenApplication { process: "code.exe".into() };
        assert!(kind.description().contains("code.exe"));
    }
}
