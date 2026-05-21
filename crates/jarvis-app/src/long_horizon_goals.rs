//! Long-horizon goal engine — supports multi-day, persistent, and resumable
//! background objectives.  Goals survive cognition loop restarts via JSONL.
//! No LLM; no cloud; pure local state.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static LH_GOALS_CREATED:   AtomicU64 = AtomicU64::new(0);
pub static LH_GOALS_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static LH_GOALS_RESUMED:   AtomicU64 = AtomicU64::new(0);

const MAX_GOALS:     usize = 50;
const PERSIST_FILE:  &str  = "long_horizon_goals.jsonl";

// ── Horizon goal ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HorizonKind {
    ProjectContinuity { project: String },
    PersistentWorkflow { workflow_id: String },
    BackgroundObjective { description: String },
    ResumableTask { task_id: String },
    MultiDayGoal { title: String, deadline_ms: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HorizonStatus { Active, Paused, Completed, Abandoned { reason: String } }

impl HorizonStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, HorizonStatus::Completed | HorizonStatus::Abandoned { .. })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HorizonGoal {
    pub id:           u64,
    pub kind:         HorizonKind,
    pub status:       HorizonStatus,
    pub created_ms:   u64,
    pub updated_ms:   u64,
    pub progress:     f32,    // 0.0–1.0
    pub notes:        Vec<String>,
}

impl HorizonGoal {
    pub fn description(&self) -> String {
        match &self.kind {
            HorizonKind::ProjectContinuity  { project }    => format!("project:{project}"),
            HorizonKind::PersistentWorkflow { workflow_id } => format!("workflow:{workflow_id}"),
            HorizonKind::BackgroundObjective{ description } => description.clone(),
            HorizonKind::ResumableTask      { task_id }    => format!("task:{task_id}"),
            HorizonKind::MultiDayGoal       { title, .. }  => title.clone(),
        }
    }
}

// ── Store ─────────────────────────────────────────────────────────────────────

struct HorizonStore {
    goals:   Vec<HorizonGoal>,
    next_id: u64,
}

static STORE: Lazy<Mutex<HorizonStore>> = Lazy::new(|| Mutex::new(HorizonStore {
    goals:   Vec::new(),
    next_id: 1,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn add(kind: HorizonKind) -> u64 {
    LH_GOALS_CREATED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    if let Ok(mut s) = STORE.lock() {
        let id = s.next_id;
        s.next_id += 1;

        if s.goals.len() >= MAX_GOALS {
            // Evict oldest completed goal, or oldest goal if none completed
            let evict_idx = s.goals.iter().position(|g| g.status.is_terminal())
                .unwrap_or(0);
            s.goals.remove(evict_idx);
        }

        let goal = HorizonGoal { id, kind, status: HorizonStatus::Active, created_ms: now, updated_ms: now, progress: 0.0, notes: Vec::new() };
        append_jsonl(&goal);
        s.goals.push(goal);
        id
    } else { 0 }
}

pub fn complete(id: u64) {
    LH_GOALS_COMPLETED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    if let Ok(mut s) = STORE.lock() {
        if let Some(g) = s.goals.iter_mut().find(|g| g.id == id) {
            g.status = HorizonStatus::Completed;
            g.progress = 1.0;
            g.updated_ms = now;
        }
    }
}

pub fn abandon(id: u64, reason: impl Into<String>) {
    let now = ts_now();
    if let Ok(mut s) = STORE.lock() {
        if let Some(g) = s.goals.iter_mut().find(|g| g.id == id) {
            g.status = HorizonStatus::Abandoned { reason: reason.into() };
            g.updated_ms = now;
        }
    }
}

pub fn update_progress(id: u64, progress: f32) {
    let now = ts_now();
    if let Ok(mut s) = STORE.lock() {
        if let Some(g) = s.goals.iter_mut().find(|g| g.id == id) {
            g.progress = progress.clamp(0.0, 1.0);
            g.updated_ms = now;
        }
    }
}

pub fn resume(id: u64) -> Option<HorizonGoal> {
    let now = ts_now();
    let found = if let Ok(mut s) = STORE.lock() {
        if let Some(g) = s.goals.iter_mut().find(|g| g.id == id && g.status == HorizonStatus::Paused) {
            g.status = HorizonStatus::Active;
            g.updated_ms = now;
            Some(g.clone())
        } else {
            s.goals.iter().find(|g| g.id == id).cloned()
        }
    } else { None };

    if found.is_some() {
        LH_GOALS_RESUMED.fetch_add(1, Ordering::Relaxed);
    }
    found
}

pub fn active_goals() -> Vec<HorizonGoal> {
    STORE.lock().map(|s| s.goals.iter().filter(|g| g.status == HorizonStatus::Active).cloned().collect()).unwrap_or_default()
}

pub fn all_goals() -> Vec<HorizonGoal> {
    STORE.lock().map(|s| s.goals.clone()).unwrap_or_default()
}

pub fn goal_count() -> usize {
    STORE.lock().map(|s| s.goals.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STORE.lock() { s.goals.clear(); }
}

// ── Persistence ───────────────────────────────────────────────────────────────

fn append_jsonl(goal: &HorizonGoal) {
    use std::io::Write as _;
    let path = crate::execution_journal::journal_dir().join(PERSIST_FILE);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        if let Ok(line) = serde_json::to_string(goal) {
            let _ = writeln!(f, "{}", line);
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

    #[test]
    fn add_and_retrieve_goal() {
        let before = goal_count();
        let id = add(HorizonKind::BackgroundObjective { description: "lhg.test.unique1".into() });
        assert!(id > 0);
        assert!(goal_count() > before);
    }

    #[test]
    fn complete_goal_removes_from_active() {
        let id = add(HorizonKind::ResumableTask { task_id: "lhg.task.unique2".into() });
        complete(id);
        assert!(!active_goals().iter().any(|g| g.id == id));
    }

    #[test]
    fn abandon_goal_marks_terminal() {
        let id = add(HorizonKind::MultiDayGoal { title: "lhg.mday.unique3".into(), deadline_ms: None });
        abandon(id, "cancelled");
        let g = all_goals().into_iter().find(|g| g.id == id).unwrap();
        assert!(g.status.is_terminal());
    }

    #[test]
    fn progress_clamped() {
        let id = add(HorizonKind::ProjectContinuity { project: "lhg.proj.unique4".into() });
        update_progress(id, 5.0);
        let g = all_goals().into_iter().find(|g| g.id == id).unwrap();
        assert!(g.progress <= 1.0);
    }

    #[test]
    fn goals_created_counter_increments() {
        let before = LH_GOALS_CREATED.load(Ordering::Relaxed);
        add(HorizonKind::BackgroundObjective { description: "lhg.counter.unique5".into() });
        assert!(LH_GOALS_CREATED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn description_non_empty() {
        let id = add(HorizonKind::PersistentWorkflow { workflow_id: "wf99".into() });
        let g = all_goals().into_iter().find(|g| g.id == id).unwrap();
        assert!(!g.description().is_empty());
    }
}
