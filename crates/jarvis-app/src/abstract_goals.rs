//! Abstract goal engine — manages conceptual objectives, generalized optimization
//! goals, strategic abstractions, and transferable long-horizon goals.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static GOALS_ADDED:     AtomicU64 = AtomicU64::new(0);
pub static GOALS_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static GOALS_ACTIVE:    AtomicU64 = AtomicU64::new(0);

const MAX_GOALS: usize = 100;

// ── GoalKind ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GoalKind {
    ConceptualObjective,
    GeneralizedOptimization,
    StrategicAbstraction,
    TransferableHorizonGoal,
}

// ── AbstractGoalStatus ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AbstractGoalStatus {
    Active,
    Completed,
    Abandoned { reason: String },
    Transferring { to_context: String },
}

// ── AbstractGoal ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AbstractGoal {
    pub id:          u64,
    pub kind:        GoalKind,
    pub description: String,
    pub priority:    f32,
    pub horizon_days: u32,
    pub progress:    f32,    // 0–1
    pub status:      AbstractGoalStatus,
    pub linked_concept: Option<String>,
    pub ts_ms:       u64,
}

impl AbstractGoal {
    pub fn is_active(&self) -> bool { self.status == AbstractGoalStatus::Active }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct GoalState {
    goals: Vec<AbstractGoal>,
    seq:   u64,
}

static STATE: Lazy<Mutex<GoalState>> = Lazy::new(|| Mutex::new(GoalState {
    goals: Vec::new(),
    seq:   0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn add(description: impl Into<String>, kind: GoalKind, priority: f32, horizon_days: u32)
    -> u64
{
    GOALS_ADDED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    let mut id = 0;
    if let Ok(mut s) = STATE.lock() {
        if s.goals.len() >= MAX_GOALS {
            // Evict lowest-priority completed goal
            if let Some(pos) = s.goals.iter().position(|g| !g.is_active()) {
                s.goals.remove(pos);
            } else {
                s.goals.remove(0);
            }
        }
        s.seq += 1;
        id = s.seq;
        s.goals.push(AbstractGoal {
            id, kind,
            description: description.into(),
            priority: priority.clamp(0.0, 1.0),
            horizon_days,
            progress: 0.0,
            status: AbstractGoalStatus::Active,
            linked_concept: None,
            ts_ms: now,
        });
        GOALS_ACTIVE.fetch_add(1, Ordering::Relaxed);
    }
    id
}

pub fn complete(id: u64) {
    if let Ok(mut s) = STATE.lock() {
        for g in s.goals.iter_mut() {
            if g.id == id && g.is_active() {
                g.status   = AbstractGoalStatus::Completed;
                g.progress = 1.0;
                g.ts_ms    = ts_now();
                GOALS_COMPLETED.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }
}

pub fn update_progress(id: u64, progress: f32) {
    if let Ok(mut s) = STATE.lock() {
        if let Some(g) = s.goals.iter_mut().find(|g| g.id == id) {
            g.progress = progress.clamp(0.0, 1.0);
            g.ts_ms    = ts_now();
            if g.progress >= 1.0 {
                g.status = AbstractGoalStatus::Completed;
                GOALS_COMPLETED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

pub fn link_concept(id: u64, concept_label: impl Into<String>) {
    if let Ok(mut s) = STATE.lock() {
        if let Some(g) = s.goals.iter_mut().find(|g| g.id == id) {
            g.linked_concept = Some(concept_label.into());
        }
    }
}

pub fn active() -> Vec<AbstractGoal> {
    STATE.lock()
        .map(|s| s.goals.iter().filter(|g| g.is_active()).cloned().collect())
        .unwrap_or_default()
}

pub fn snapshot() -> Vec<AbstractGoal> {
    STATE.lock().map(|s| s.goals.clone()).unwrap_or_default()
}

/// Tick all active goals: increment progress and complete those ≥ 1.0.
pub fn tick_progress(delta: f32) {
    if let Ok(mut s) = STATE.lock() {
        for g in s.goals.iter_mut() {
            if !g.is_active() { continue; }
            g.progress = (g.progress + delta).min(1.0);
            if g.progress >= 1.0 {
                g.status = AbstractGoalStatus::Completed;
                GOALS_COMPLETED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
