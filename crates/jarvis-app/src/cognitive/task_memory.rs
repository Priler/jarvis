#![allow(dead_code)]

use std::path::PathBuf;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub const TASK_MEMORY_MAX: usize = 10;
const EXPIRY_PENDING_MS: u64 = 48 * 3600 * 1000;
const EXPIRY_IN_PROGRESS_MS: u64 = 4 * 3600 * 1000;
const EXPIRY_BLOCKED_MS: u64 = 24 * 3600 * 1000;

// ── Task status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Blocked { reason: String },
    Complete,
    Expired,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Blocked { .. } => "blocked",
            TaskStatus::Complete => "complete",
            TaskStatus::Expired => "expired",
        }
    }
}

// ── Pending task ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTask {
    pub id: String,
    pub goal: String,
    pub step_count: usize,
    pub completed_steps: usize,
    pub created_ms: u64,
    pub last_active_ms: u64,
    pub expiry_ms: u64,
    pub status: TaskStatus,
}

impl PendingTask {
    pub fn new(id: impl Into<String>, goal: impl Into<String>, step_count: usize) -> Self {
        let now = now_ms();
        Self {
            id: id.into(),
            goal: goal.into(),
            step_count,
            completed_steps: 0,
            created_ms: now,
            last_active_ms: now,
            expiry_ms: now + EXPIRY_PENDING_MS,
            status: TaskStatus::Pending,
        }
    }

    pub fn start(&mut self) {
        self.status = TaskStatus::InProgress;
        self.last_active_ms = now_ms();
        self.expiry_ms = self.last_active_ms + EXPIRY_IN_PROGRESS_MS;
    }

    pub fn complete_step(&mut self) {
        self.completed_steps = (self.completed_steps + 1).min(self.step_count);
        self.last_active_ms = now_ms();
        if self.completed_steps >= self.step_count {
            self.status = TaskStatus::Complete;
        }
    }

    pub fn block(&mut self, reason: impl Into<String>) {
        self.status = TaskStatus::Blocked { reason: reason.into() };
        self.last_active_ms = now_ms();
        self.expiry_ms = self.last_active_ms + EXPIRY_BLOCKED_MS;
    }

    pub fn progress_pct(&self) -> u8 {
        if self.step_count == 0 { return 100; }
        ((self.completed_steps * 100) / self.step_count) as u8
    }

    pub fn is_expired(&self) -> bool {
        now_ms() > self.expiry_ms
    }
}

// ── Task memory ───────────────────────────────────────────────────────────────

pub struct TaskMemory {
    tasks: Vec<PendingTask>,
    path: PathBuf,
}

impl TaskMemory {
    pub fn new(path: PathBuf) -> Self {
        let tasks = Self::load_from(&path);
        Self { tasks, path }
    }

    fn load_from(path: &PathBuf) -> Vec<PendingTask> {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&self.tasks) {
            let _ = std::fs::write(&self.path, content);
        }
    }

    pub fn push(&mut self, task: PendingTask) {
        // Evict oldest pending task if at capacity.
        if self.tasks.len() >= TASK_MEMORY_MAX {
            if let Some(pos) = self.tasks.iter().position(|t| t.status == TaskStatus::Pending) {
                self.tasks.remove(pos);
            } else {
                self.tasks.remove(0);
            }
        }
        self.tasks.push(task);
        self.save();
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut PendingTask> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&PendingTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn remove_complete(&mut self) {
        self.tasks.retain(|t| t.status != TaskStatus::Complete);
        self.save();
    }

    /// Expire tasks whose deadline has passed. Called by watchdog.
    pub fn expire_stale(&mut self) -> usize {
        let mut count = 0;
        for task in self.tasks.iter_mut() {
            if task.is_expired() && !matches!(task.status, TaskStatus::Complete | TaskStatus::Expired) {
                task.status = TaskStatus::Expired;
                count += 1;
            }
        }
        if count > 0 {
            warn!("[TASK_MEMORY] Expired {} stale task(s)", count);
            self.save();
        }
        count
    }

    pub fn active_tasks(&self) -> Vec<&PendingTask> {
        self.tasks.iter()
            .filter(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::InProgress | TaskStatus::Blocked { .. }))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.tasks.len()
    }
}
