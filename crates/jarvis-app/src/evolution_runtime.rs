//! Evolution runtime — background thread that drives the full self-evolving
//! cognition cycle: topology refresh → rebalance → route → schedule → restructure
//! → self-optimise → validate → journal.  Runs every 3 500 ms.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub static EVO_TICKS_TOTAL:   AtomicU64 = AtomicU64::new(0);
pub static EVO_TICKS_SKIPPED: AtomicU64 = AtomicU64::new(0);
pub static EVO_ERRORS:        AtomicU64 = AtomicU64::new(0);

static EVO_RUNNING: AtomicBool = AtomicBool::new(false);
static EVO_STOP:    AtomicBool = AtomicBool::new(false);

const TICK_INTERVAL_MS: u64 = 3_500;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── EvolutionTickResult ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EvolutionTickResult {
    pub tick_id:             u64,
    pub steps_completed:     usize,
    pub restructurings:      usize,
    pub routing_path:        &'static str,
    pub optimization_target: Option<&'static str>,
    pub validator_approved:  bool,
    pub duration_ms:         u64,
}

// ── Tick logic ────────────────────────────────────────────────────────────────

pub fn run_tick() -> EvolutionTickResult {
    let start_ms = ts_now();
    let mut steps = 0usize;

    EVO_TICKS_TOTAL.fetch_add(1, Ordering::Relaxed);

    // ── Step 1: Refresh adaptive topology loads ───────────────────────────────
    crate::adaptive_topology::refresh_loads();
    steps += 1;

    // ── Step 2: Rebalance routing weights ─────────────────────────────────────
    crate::adaptive_topology::rebalance();
    steps += 1;

    // ── Step 3: Dynamic routing decision ─────────────────────────────────────
    let decision = crate::dynamic_reasoning_router::route();
    let routing_path = decision.chosen_path.name();
    steps += 1;

    // ── Step 4: Adapt scheduler throttles ─────────────────────────────────────
    crate::adaptive_scheduler::adapt();
    steps += 1;

    // ── Step 5: Cognition restructuring ───────────────────────────────────────
    let restructurings = crate::cognition_restructuring::restructure();
    steps += 1;

    // ── Step 6: Self-optimization (at most one target per tick) ───────────────
    let opt = crate::self_optimization::optimize();
    let optimization_target = opt.as_ref().map(|r| r.target.label());
    steps += 1;

    // ── Step 7: Validate the evolution round ──────────────────────────────────
    let val = crate::evolution_validator::validate_change("evolution_runtime_tick");
    let validator_approved = val.is_approved();
    steps += 1;

    // ── Step 8: Journal this evolution tick ───────────────────────────────────
    let tick_id = EVO_TICKS_TOTAL.load(Ordering::Relaxed);
    crate::topology_memory::record(crate::topology_memory::TopologyEvent::TopologyChange {
        component: "evolution_runtime".into(),
        old_state: "tick_start".into(),
        new_state: format!(
            "tick_{tick_id}_path_{routing_path}_restructurings_{restructurings}"
        ),
    });
    steps += 1;

    let duration_ms = ts_now().saturating_sub(start_ms);

    EvolutionTickResult {
        tick_id,
        steps_completed: steps,
        restructurings,
        routing_path,
        optimization_target,
        validator_approved,
        duration_ms,
    }
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

pub fn start() {
    if EVO_RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    EVO_STOP.store(false, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("jarvis-evolution-runtime".to_string())
        .spawn(move || {
            while !EVO_STOP.load(Ordering::Relaxed) {
                let result = std::panic::catch_unwind(|| run_tick());
                match result {
                    Ok(_)  => {}
                    Err(_) => { EVO_ERRORS.fetch_add(1, Ordering::Relaxed); }
                }
                std::thread::sleep(Duration::from_millis(TICK_INTERVAL_MS));
            }
            EVO_RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

pub fn stop() {
    EVO_STOP.store(true, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    EVO_RUNNING.load(Ordering::Relaxed)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_tick_completes_all_steps() {
        let r = run_tick();
        assert_eq!(r.steps_completed, 8);
    }

    #[test]
    fn run_tick_has_valid_routing_path() {
        let r = run_tick();
        assert!(!r.routing_path.is_empty());
    }

    #[test]
    fn run_tick_increments_counter() {
        let before = EVO_TICKS_TOTAL.load(Ordering::Relaxed);
        let _ = run_tick();
        assert!(EVO_TICKS_TOTAL.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn stop_no_panic_when_not_running() {
        stop();
    }
}
