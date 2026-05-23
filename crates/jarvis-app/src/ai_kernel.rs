//! AI OS kernel — manages cognition services, supervises runtime lifecycles,
//! coordinates cognition layers, manages distributed execution, and maintains
//! persistent cognition state.
//!
//! Background thread at 4 000 ms.  10-step deterministic tick.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub static KERNEL_TICKS:   AtomicU64 = AtomicU64::new(0);
pub static KERNEL_ERRORS:  AtomicU64 = AtomicU64::new(0);
pub static KERNEL_SKIPPED: AtomicU64 = AtomicU64::new(0);

static KERNEL_RUNNING: AtomicBool = AtomicBool::new(false);
static KERNEL_STOP:    AtomicBool = AtomicBool::new(false);

const TICK_INTERVAL_MS: u64 = 4_000;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── KernelTickResult ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct KernelTickResult {
    pub tick_id:           u64,
    pub steps_completed:   usize,
    pub services_healthy:  usize,
    pub workers_active:    usize,
    pub recovery_actions:  usize,
    pub system_stable:     bool,
    pub continuity_score:  f32,
    pub duration_ms:       u64,
}

// ── Tick logic ────────────────────────────────────────────────────────────────

pub fn run_tick() -> KernelTickResult {
    let start_ms = ts_now();
    KERNEL_TICKS.fetch_add(1, Ordering::Relaxed);
    let tick_id  = KERNEL_TICKS.load(Ordering::Relaxed);
    let mut steps = 0usize;

    // ── Step 1: Distributed safety check ─────────────────────────────────────
    let safe = crate::distributed_safety::check_distributed_safe();
    steps += 1;
    if !safe.is_safe {
        KERNEL_SKIPPED.fetch_add(1, Ordering::Relaxed);
        crate::ai_os_observability::record(
            crate::ai_os_observability::AiOsEvent::SafetyGate {
                component: "ai_kernel".into(),
                reason: safe.reason.unwrap_or_else(|| "safety_blocked".into()),
            }
        );
        return KernelTickResult {
            tick_id, steps_completed: steps,
            services_healthy: 0, workers_active: 0,
            recovery_actions: 0, system_stable: false,
            continuity_score: 0.0,
            duration_ms: ts_now().saturating_sub(start_ms),
        };
    }

    // ── Step 2: Update resource governor ─────────────────────────────────────
    let res = crate::resource_governor::sample();
    steps += 1;

    // ── Step 3: Sync distributed memory bus ──────────────────────────────────
    let bus_state = crate::distributed_memory_bus::sync();
    crate::distributed_memory_bus::propagate();
    steps += 1;

    // ── Step 4: Update persistent services ───────────────────────────────────
    let services = crate::persistent_services::check_all();
    let services_healthy = services.iter().filter(|s| s.is_healthy).count();
    steps += 1;

    // ── Step 5: Cognitive process manager tick ────────────────────────────────
    let proc_report = crate::cognitive_process_manager::supervise_tick();
    steps += 1;

    // ── Step 6: Recursive orchestrator ───────────────────────────────────────
    let _orch = crate::recursive_orchestrator::orchestrate();
    steps += 1;

    // ── Step 7: Distributed runtime rebalance ─────────────────────────────────
    if res.should_throttle {
        crate::distributed_runtime::rebalance();
    } else {
        let _ = crate::distributed_runtime::dispatch("kernel_workload", res.cognition_load);
    }
    let workers_active = crate::distributed_runtime::active_workers();
    steps += 1;

    // ── Step 8: Long-run cognition maintenance ────────────────────────────────
    let continuity = crate::long_run_cognition::maintain();
    steps += 1;

    // ── Step 9: Autonomous recovery if needed ────────────────────────────────
    let stab = crate::recursive_stability::check();
    let recovery_actions = if !stab.overall_stable || proc_report.degraded_services > 0 {
        let r = crate::autonomous_recovery::recover();
        r.total_actions
    } else { 0 };
    steps += 1;

    // ── Step 10: Journal ──────────────────────────────────────────────────────
    crate::ai_os_observability::record(
        crate::ai_os_observability::AiOsEvent::KernelTick {
            tick_id,
            steps,
        }
    );

    // Suppress bus_state warning — used for coherence check
    let _coherent = bus_state.is_coherent();
    steps += 1;

    KernelTickResult {
        tick_id,
        steps_completed:  steps,
        services_healthy,
        workers_active,
        recovery_actions,
        system_stable:    continuity.is_continuous() && stab.overall_stable,
        continuity_score: continuity.overall_continuity,
        duration_ms:      ts_now().saturating_sub(start_ms),
    }
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

pub fn start() {
    if KERNEL_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    KERNEL_STOP.store(false, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("jarvis-ai-kernel".to_string())
        .spawn(move || {
            while !KERNEL_STOP.load(Ordering::Relaxed) {
                let result = std::panic::catch_unwind(|| run_tick());
                if result.is_err() {
                    KERNEL_ERRORS.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(TICK_INTERVAL_MS));
            }
            KERNEL_RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

pub fn stop() {
    KERNEL_STOP.store(true, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    KERNEL_RUNNING.load(Ordering::Relaxed)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_tick_completes() {
        let r = run_tick();
        assert!(r.steps_completed >= 1);
    }

    #[test]
    fn run_tick_increments_counter() {
        let before = KERNEL_TICKS.load(Ordering::Relaxed);
        let _ = run_tick();
        assert!(KERNEL_TICKS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn continuity_score_bounded() {
        let r = run_tick();
        assert!(r.continuity_score >= 0.0 && r.continuity_score <= 1.0);
    }

    #[test]
    fn stop_no_panic_when_not_running() {
        stop();
    }
}
