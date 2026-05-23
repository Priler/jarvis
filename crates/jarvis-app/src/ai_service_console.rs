//! AI service console — provides status snapshots of active cognition services,
//! distributed workers, recursion depth, scheduler state, degraded runtimes,
//! and recovery actions.

use std::sync::atomic::{AtomicU64, Ordering};

pub static CONSOLE_SNAPSHOTS: AtomicU64 = AtomicU64::new(0);

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ConsoleSnapshot ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConsoleSnapshot {
    pub active_services:     usize,
    pub degraded_services:   usize,
    pub active_workers:      usize,
    pub avg_worker_load:     f32,
    pub recursion_depth:     u32,
    pub active_jobs:         usize,
    pub recovery_actions:    u64,
    pub continuity_score:    f32,
    pub system_stable:       bool,
    pub safety_gates_fired:  u64,
    pub ts_ms:               u64,
}

impl ConsoleSnapshot {
    pub fn overall_health(&self) -> f32 {
        let service_health = if self.active_services == 0 { 0.0 }
            else { self.active_services as f32
                / (self.active_services + self.degraded_services) as f32 };
        let worker_health = 1.0 - self.avg_worker_load;
        (service_health * 0.40 + worker_health * 0.30 + self.continuity_score * 0.30)
            .clamp(0.0, 1.0)
    }
}

// ── API ───────────────────────────────────────────────────────────────────────

/// Capture a full system status snapshot.
pub fn snapshot() -> ConsoleSnapshot {
    let services     = crate::persistent_services::all_services();
    let healthy      = services.iter().filter(|s| s.is_healthy).count();
    let degraded     = services.len() - healthy;
    let workers      = crate::distributed_runtime::active_workers();
    let worker_load  = crate::distributed_runtime::avg_worker_load();
    let depth        = crate::recursive_stability::current_depth();
    let active_jobs  = crate::ai_task_scheduler::active_job_count();
    let recovery     = crate::autonomous_recovery::total_recovery_actions();
    let safety_gates = crate::distributed_safety::gates_fired();
    let stab         = crate::recursive_stability::check();

    // Continuity score from long_run_cognition
    let continuity = crate::long_run_cognition::avg_continuity(5);

    let snap = ConsoleSnapshot {
        active_services:   healthy,
        degraded_services: degraded,
        active_workers:    workers,
        avg_worker_load:   worker_load,
        recursion_depth:   depth,
        active_jobs,
        recovery_actions:  recovery,
        continuity_score:  continuity,
        system_stable:     stab.overall_stable,
        safety_gates_fired: safety_gates,
        ts_ms:             ts_now(),
    };

    CONSOLE_SNAPSHOTS.fetch_add(1, Ordering::Relaxed);
    snap
}

pub fn print_status(snap: &ConsoleSnapshot) -> String {
    format!(
        "[AI_OS_CONSOLE] services={}/{} workers={} load={:.2} depth={} jobs={} stable={} health={:.2}",
        snap.active_services,
        snap.active_services + snap.degraded_services,
        snap.active_workers,
        snap.avg_worker_load,
        snap.recursion_depth,
        snap.active_jobs,
        snap.system_stable,
        snap.overall_health(),
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.overall_health() >= 0.0 && s.overall_health() <= 1.0);
    }

    #[test]
    fn print_status_non_empty() {
        let s = snapshot();
        let out = print_status(&s);
        assert!(!out.is_empty());
        assert!(out.contains("[AI_OS_CONSOLE]"));
    }
}
