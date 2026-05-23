//! Cognitive process manager — spawns cognition services, supervises workers,
//! restarts degraded services, isolates unstable cognition pipelines, and
//! prioritises critical runtime processes.

use std::sync::atomic::{AtomicU64, Ordering};

pub static PROCESSES_SUPERVISED: AtomicU64 = AtomicU64::new(0);
pub static PROCESSES_RESTARTED:  AtomicU64 = AtomicU64::new(0);
pub static PIPELINES_ISOLATED:   AtomicU64 = AtomicU64::new(0);

// ── ProcessReport ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProcessReport {
    pub total_services:     usize,
    pub healthy_services:   usize,
    pub degraded_services:  usize,
    pub restarted_this_tick: usize,
    pub isolated_pipelines: usize,
    pub active_jobs:        usize,
}

// ── Tick logic ────────────────────────────────────────────────────────────────

/// Supervise all cognition services for this tick.
pub fn supervise_tick() -> ProcessReport {
    // 1. Check all persistent services
    let services = crate::persistent_services::check_all();
    let total = services.len();
    let healthy = services.iter().filter(|s| s.is_healthy).count();
    let degraded = total - healthy;

    PROCESSES_SUPERVISED.fetch_add(total as u64, Ordering::Relaxed);

    // 2. Attempt recovery of degraded services
    let restarted = if degraded > 0 {
        let r = crate::persistent_services::recover_degraded();
        PROCESSES_RESTARTED.fetch_add(r as u64, Ordering::Relaxed);
        if r > 0 {
            crate::ai_os_observability::record(
                crate::ai_os_observability::AiOsEvent::RecoveryAction {
                    component: "process_manager".into(),
                    action: format!("restarted_{r}_services"),
                }
            );
        }
        r
    } else { 0 };

    // 3. Isolate unstable pipelines in task scheduler
    let stab = crate::recursive_stability::check();
    let isolated = if stab.risk_score > 0.60 {
        let n = crate::ai_task_scheduler::isolate_unstable(0.50);
        PIPELINES_ISOLATED.fetch_add(n as u64, Ordering::Relaxed);
        n
    } else { 0 };

    // 4. Auto-schedule maintenance jobs
    crate::ai_task_scheduler::auto_schedule();

    ProcessReport {
        total_services:      total,
        healthy_services:    healthy,
        degraded_services:   degraded,
        restarted_this_tick: restarted,
        isolated_pipelines:  isolated,
        active_jobs:         crate::ai_task_scheduler::active_job_count(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervise_tick_no_panic() {
        let r = supervise_tick();
        assert_eq!(r.total_services, 7);
    }

    #[test]
    fn healthy_services_non_negative() {
        let r = supervise_tick();
        assert!(r.healthy_services <= r.total_services);
    }
}
