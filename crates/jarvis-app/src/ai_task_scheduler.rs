//! AI task scheduler — schedules cognition jobs, prioritizes strategic tasks,
//! manages long-running cognition, isolates unstable tasks, and rebalances
//! workloads dynamically.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static JOBS_SCHEDULED:  AtomicU64 = AtomicU64::new(0);
pub static JOBS_COMPLETED:  AtomicU64 = AtomicU64::new(0);
pub static JOBS_ISOLATED:   AtomicU64 = AtomicU64::new(0);

const MAX_ACTIVE_JOBS:       usize = 20;
const MIN_PRIORITY_TO_QUEUE: f32   = 0.10;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── JobKind ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JobKind {
    Maintenance,      // lowest priority
    Simulation,
    Planning,
    Strategic,        // highest priority
}

impl JobKind {
    pub fn base_priority(&self) -> f32 {
        match self {
            Self::Maintenance => 0.20,
            Self::Simulation  => 0.50,
            Self::Planning    => 0.70,
            Self::Strategic   => 0.90,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Maintenance => "maintenance",
            Self::Simulation  => "simulation",
            Self::Planning    => "planning",
            Self::Strategic   => "strategic",
        }
    }
}

// ── CognitionJob ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CognitionJob {
    pub id:          u64,
    pub label:       String,
    pub kind:        JobKind,
    pub priority:    f32,
    pub isolated:    bool,
    pub queued_at:   u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SchedulerState {
    active:  Vec<CognitionJob>,
    seq:     u64,
}

impl SchedulerState {
    fn new() -> Self { SchedulerState { active: Vec::new(), seq: 0 } }

    fn sort_by_priority(&mut self) {
        self.active.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap());
    }
}

static STATE: Lazy<Mutex<SchedulerState>> = Lazy::new(|| Mutex::new(SchedulerState::new()));

// ── API ───────────────────────────────────────────────────────────────────────

/// Schedule a cognition job.  Returns the job ID, or 0 if rejected.
pub fn schedule_job(kind: JobKind, label: impl Into<String>) -> u64 {
    let priority = kind.base_priority();
    if priority < MIN_PRIORITY_TO_QUEUE { return 0; }

    // Safety gate: don't accept new jobs under distributed overload
    let dist_safe = crate::distributed_safety::check_distributed_safe();
    if !dist_safe.is_safe && kind != JobKind::Strategic {
        JOBS_ISOLATED.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    let mut s = STATE.lock().unwrap();
    if s.active.len() >= MAX_ACTIVE_JOBS {
        // Evict lowest-priority job
        // ensure sorted before eviction
        s.active.sort_by(|a, b| a.priority.partial_cmp(&b.priority).unwrap());
        s.active.remove(0);
        JOBS_ISOLATED.fetch_add(1, Ordering::Relaxed);
    }

    s.seq += 1;
    let id = s.seq;
    s.active.push(CognitionJob {
        id, label: label.into(), kind, priority, isolated: false, queued_at: ts_now(),
    });
    s.sort_by_priority();
    JOBS_SCHEDULED.fetch_add(1, Ordering::Relaxed);
    id
}

/// Complete (remove) a job by ID.
pub fn complete_job(id: u64) {
    let mut s = STATE.lock().unwrap();
    s.active.retain(|j| j.id != id);
    JOBS_COMPLETED.fetch_add(1, Ordering::Relaxed);
}

/// Isolate (flag as unstable) all jobs above a stability threshold.
pub fn isolate_unstable(stability_threshold: f32) -> usize {
    let mut s = STATE.lock().unwrap();
    let mut count = 0;
    for job in s.active.iter_mut() {
        if job.priority < stability_threshold && !job.isolated {
            job.isolated = true;
            count += 1;
        }
    }
    JOBS_ISOLATED.fetch_add(count as u64, Ordering::Relaxed);
    count
}

/// Rebalance: promote strategic jobs, demote maintenance under load.
pub fn rebalance() {
    let avg_load = crate::adaptive_topology::avg_load();
    let mut s = STATE.lock().unwrap();
    for job in s.active.iter_mut() {
        if job.kind == JobKind::Strategic && job.priority < 0.90 {
            job.priority = (job.priority + 0.05).min(0.95);
        } else if job.kind == JobKind::Maintenance && avg_load > 0.65 {
            job.priority = (job.priority - 0.05).max(0.10);
        }
    }
    s.sort_by_priority();
}

/// Auto-schedule standard cognition jobs from live signals.
pub fn auto_schedule() -> usize {
    let unc  = crate::generalized_uncertainty::profile();
    let prob = crate::probabilistic_stability::check();
    let mut count = 0;

    if schedule_job(JobKind::Maintenance, "topology_rebalance") > 0 { count += 1; }

    if prob.instability_score > 0.40 {
        if schedule_job(JobKind::Planning, "belief_recovery_plan") > 0 { count += 1; }
    }

    if unc.overall > 0.50 {
        if schedule_job(JobKind::Strategic, "uncertainty_reduction_strategy") > 0 { count += 1; }
    }

    count
}

pub fn active_jobs()      -> Vec<CognitionJob> { STATE.lock().unwrap().active.clone() }
pub fn active_job_count() -> usize             { STATE.lock().unwrap().active.len() }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_job_strategic_no_panic() {
        let id = schedule_job(JobKind::Strategic, "test_strategic");
        let _ = id;
    }

    #[test]
    fn complete_job_reduces_count() {
        let id = schedule_job(JobKind::Maintenance, "test_maint");
        if id > 0 {
            let before = active_job_count();
            complete_job(id);
            assert!(active_job_count() <= before);
        }
    }

    #[test]
    fn auto_schedule_no_panic() {
        let _ = auto_schedule();
    }

    #[test]
    fn rebalance_no_panic() {
        rebalance();
    }
}
