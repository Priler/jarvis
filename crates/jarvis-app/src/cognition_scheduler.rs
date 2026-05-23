//! Cognition scheduler — priority queue for cognitive tasks within a tick.
//!
//! Tasks are enqueued by priority; the scheduler drains them in order during
//! the Plan/Act phases of the cognition loop.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static TASKS_ENQUEUED:  AtomicU64 = AtomicU64::new(0);
pub static TASKS_DISPATCHED: AtomicU64 = AtomicU64::new(0);
pub static TASKS_DROPPED:   AtomicU64 = AtomicU64::new(0);

const MAX_QUEUE_DEPTH: usize = 64;

// ── Task kinds ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CognitionTaskKind {
    ObserveEnvironment,
    UpdateWorldModel,
    DetectAnomalies,
    EvaluateGoals,
    GeneratePredictions,
    LearnWorkflowPattern,
    ReflectOnFailures,
    RefreshAttention,
    CheckContinuity,
    RunSafetyCheck,
    LogObservability,
}

// ── Priority ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum TaskPriority {
    Critical = 0,
    High     = 1,
    Normal   = 2,
    Low      = 3,
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// ── Cognition task ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CognitionTask {
    pub id:       u64,
    pub kind:     CognitionTaskKind,
    pub priority: TaskPriority,
    pub note:     Option<String>,
    pub ts_ms:    u64,
}

impl CognitionTask {
    pub fn new(kind: CognitionTaskKind, priority: TaskPriority) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self {
            id: COUNTER.fetch_add(1, Ordering::Relaxed),
            kind,
            priority,
            note: None,
            ts_ms: ts_now(),
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

// ── Scheduler ─────────────────────────────────────────────────────────────────

static QUEUE: Lazy<Mutex<Vec<CognitionTask>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub struct CognitionScheduler;

impl CognitionScheduler {
    /// Enqueue a task. Drops if queue is full.
    pub fn enqueue(task: CognitionTask) -> bool {
        let mut guard = match QUEUE.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if guard.len() >= MAX_QUEUE_DEPTH {
            TASKS_DROPPED.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        guard.push(task);
        guard.sort_by_key(|t| t.priority);
        TASKS_ENQUEUED.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Pop the highest-priority task.
    pub fn next() -> Option<CognitionTask> {
        let mut guard = QUEUE.lock().ok()?;
        if guard.is_empty() {
            return None;
        }
        let task = guard.remove(0);
        TASKS_DISPATCHED.fetch_add(1, Ordering::Relaxed);
        Some(task)
    }

    /// Drain all tasks (returns them sorted by priority, highest first).
    pub fn drain_all() -> Vec<CognitionTask> {
        let mut guard = match QUEUE.lock() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        let tasks: Vec<_> = guard.drain(..).collect();
        TASKS_DISPATCHED.fetch_add(tasks.len() as u64, Ordering::Relaxed);
        tasks
    }

    pub fn pending_count() -> usize {
        QUEUE.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty() -> bool {
        Self::pending_count() == 0
    }

    /// Enqueue a full default tick schedule.
    pub fn enqueue_tick_schedule() {
        use CognitionTaskKind::*;
        use TaskPriority::*;
        let tasks = [
            (RunSafetyCheck,         Critical),
            (ObserveEnvironment,     High),
            (UpdateWorldModel,       High),
            (DetectAnomalies,        High),
            (EvaluateGoals,          Normal),
            (RefreshAttention,       Normal),
            (GeneratePredictions,    Normal),
            (CheckContinuity,        Normal),
            (LearnWorkflowPattern,   Low),
            (ReflectOnFailures,      Low),
            (LogObservability,       Low),
        ];
        for (kind, priority) in tasks {
            Self::enqueue(CognitionTask::new(kind, priority));
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

    fn clear_queue() {
        CognitionScheduler::drain_all();
    }

    #[test]
    fn enqueue_and_dequeue_preserves_priority_order() {
        clear_queue();
        CognitionScheduler::enqueue(CognitionTask::new(CognitionTaskKind::ReflectOnFailures, TaskPriority::Low));
        CognitionScheduler::enqueue(CognitionTask::new(CognitionTaskKind::RunSafetyCheck,    TaskPriority::Critical));
        CognitionScheduler::enqueue(CognitionTask::new(CognitionTaskKind::EvaluateGoals,     TaskPriority::Normal));

        let first = CognitionScheduler::next().unwrap();
        assert_eq!(first.priority, TaskPriority::Critical);
    }

    #[test]
    fn drain_all_empties_queue() {
        clear_queue();
        CognitionScheduler::enqueue(CognitionTask::new(CognitionTaskKind::ObserveEnvironment, TaskPriority::Normal));
        let drained = CognitionScheduler::drain_all();
        assert!(!drained.is_empty());
        assert!(CognitionScheduler::is_empty());
    }

    #[test]
    fn enqueue_tick_schedule_populates_queue() {
        clear_queue();
        CognitionScheduler::enqueue_tick_schedule();
        assert!(!CognitionScheduler::is_empty());
        clear_queue();
    }

    #[test]
    fn task_with_note_stores_note() {
        let t = CognitionTask::new(CognitionTaskKind::LearnWorkflowPattern, TaskPriority::Low)
            .with_note("open IDE pattern");
        assert_eq!(t.note.as_deref(), Some("open IDE pattern"));
    }

    #[test]
    fn tasks_enqueued_counter_increments() {
        clear_queue();
        let before = TASKS_ENQUEUED.load(Ordering::Relaxed);
        CognitionScheduler::enqueue(CognitionTask::new(CognitionTaskKind::LogObservability, TaskPriority::Low));
        assert!(TASKS_ENQUEUED.load(Ordering::Relaxed) > before);
        clear_queue();
    }

    #[test]
    fn priority_ordering_is_correct() {
        assert!(TaskPriority::Critical < TaskPriority::High);
        assert!(TaskPriority::High     < TaskPriority::Normal);
        assert!(TaskPriority::Normal   < TaskPriority::Low);
    }
}
