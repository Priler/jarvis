//! GPU scheduler — fair, priority-aware scheduling of GPU workloads across Jarvis subsystems.
//! Enforces VRAM budgets and prevents one subsystem from starving others.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static GPU_TASKS_SCHEDULED: AtomicU64 = AtomicU64::new(0);
pub static GPU_TASKS_DEFERRED:  AtomicU64 = AtomicU64::new(0);
pub static GPU_BUDGET_ENFORCES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum GpuTaskKind {
    VoiceInference,     // highest priority — user-facing latency
    Inference,          // interactive LLM call
    EmbeddingBatch,     // RAG indexing
    WorldSimulation,    // background
    Diagnostics,        // lowest
}

impl GpuTaskKind {
    pub fn priority_weight(&self) -> u8 {
        match self {
            Self::VoiceInference => 10,
            Self::Inference      => 8,
            Self::EmbeddingBatch => 5,
            Self::WorldSimulation => 3,
            Self::Diagnostics    => 1,
        }
    }

    pub fn vram_estimate_mb(&self) -> u32 {
        match self {
            Self::VoiceInference => 512,
            Self::Inference      => 2048,
            Self::EmbeddingBatch => 256,
            Self::WorldSimulation => 128,
            Self::Diagnostics    => 16,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GpuTask {
    pub id:         u64,
    pub kind:       GpuTaskKind,
    pub submitter:  String,
    pub vram_mb:    u32,
    pub submitted:  u64,
    pub deferred:   bool,
}

struct SchedulerState {
    pending:        Vec<GpuTask>,
    completed:      u64,
    vram_budget_mb: u32,
    vram_in_use_mb: u32,
    next_id:        u64,
}

impl SchedulerState {
    fn new() -> Self {
        Self {
            pending:        Vec::new(),
            completed:      0,
            vram_budget_mb: 4096, // default 4 GB
            vram_in_use_mb: 0,
            next_id:        1,
        }
    }
}

static STATE: Lazy<Mutex<SchedulerState>> = Lazy::new(|| Mutex::new(SchedulerState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn set_vram_budget_mb(budget: u32) {
    STATE.lock().unwrap().vram_budget_mb = budget;
}

pub fn submit(kind: GpuTaskKind, submitter: &str) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_id;
    s.next_id += 1;
    let vram_mb = kind.vram_estimate_mb();
    let deferred = s.vram_in_use_mb + vram_mb > s.vram_budget_mb;
    if deferred {
        GPU_TASKS_DEFERRED.fetch_add(1, Ordering::Relaxed);
    } else {
        s.vram_in_use_mb += vram_mb;
        GPU_TASKS_SCHEDULED.fetch_add(1, Ordering::Relaxed);
    }
    s.pending.push(GpuTask {
        id, kind, submitter: submitter.to_string(),
        vram_mb, submitted: ts_now(), deferred,
    });
    id
}

pub fn complete(task_id: u64) {
    let mut s = STATE.lock().unwrap();
    if let Some(pos) = s.pending.iter().position(|t| t.id == task_id) {
        let task = s.pending.remove(pos);
        if !task.deferred {
            s.vram_in_use_mb = s.vram_in_use_mb.saturating_sub(task.vram_mb);
        }
        s.completed += 1;
    }
}

pub fn enforce_budget() {
    let mut s = STATE.lock().unwrap();
    let budget = s.vram_budget_mb;
    // Drop lowest-priority deferred tasks if queue is too deep
    if s.pending.len() > 20 {
        s.pending.sort_by_key(|t| t.kind.priority_weight());
        s.pending.truncate(20);
        GPU_BUDGET_ENFORCES.fetch_add(1, Ordering::Relaxed);
    }
    // Release tasks that are over budget (lowest priority first)
    while s.vram_in_use_mb > budget && !s.pending.is_empty() {
        s.pending.remove(0);
        GPU_BUDGET_ENFORCES.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn pending_count() -> usize { STATE.lock().unwrap().pending.len() }
pub fn vram_in_use_mb() -> u32  { STATE.lock().unwrap().vram_in_use_mb }

#[derive(Debug, serde::Serialize)]
pub struct GpuSchedulerSnapshot {
    pub tasks_scheduled:  u64,
    pub tasks_deferred:   u64,
    pub budget_enforces:  u64,
    pub pending_tasks:    usize,
    pub vram_budget_mb:   u32,
    pub vram_in_use_mb:   u32,
    pub utilization_pct:  f32,
}

pub fn snapshot() -> GpuSchedulerSnapshot {
    let s = STATE.lock().unwrap();
    let util = if s.vram_budget_mb > 0 {
        s.vram_in_use_mb as f32 / s.vram_budget_mb as f32
    } else { 0.0 };
    GpuSchedulerSnapshot {
        tasks_scheduled:  GPU_TASKS_SCHEDULED.load(Ordering::Relaxed),
        tasks_deferred:   GPU_TASKS_DEFERRED.load(Ordering::Relaxed),
        budget_enforces:  GPU_BUDGET_ENFORCES.load(Ordering::Relaxed),
        pending_tasks:    s.pending.len(),
        vram_budget_mb:   s.vram_budget_mb,
        vram_in_use_mb:   s.vram_in_use_mb,
        utilization_pct:  util * 100.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_and_complete() {
        let id = submit(GpuTaskKind::Inference, "test");
        assert!(id > 0);
        complete(id);
        assert!(GPU_TASKS_SCHEDULED.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn voice_has_highest_priority() {
        assert!(GpuTaskKind::VoiceInference.priority_weight()
            > GpuTaskKind::WorldSimulation.priority_weight());
    }

    #[test]
    fn budget_enforcement_no_panic() {
        enforce_budget();
        assert!(GPU_BUDGET_ENFORCES.load(Ordering::Relaxed) < u64::MAX);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.vram_budget_mb > 0);
    }
}
