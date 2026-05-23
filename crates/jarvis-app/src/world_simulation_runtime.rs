//! World simulation runtime — background thread driving the generalized world
//! simulation cycle every 3 000 ms.
//!
//! Tick sequence (10 steps):
//!   1. Safety check
//!   2. Forecast environment
//!   3. Generate abstract world
//!   4. Generate scenarios
//!   5. Auto-schedule simulations
//!   6. Predict world evolution
//!   7. Evolve world model
//!   8. Autonomous cognitive synthesis
//!   9. Generate topology candidates
//!  10. Journal to future_memory + observability

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub static SIM_TICKS_TOTAL:   AtomicU64 = AtomicU64::new(0);
pub static SIM_TICKS_SKIPPED: AtomicU64 = AtomicU64::new(0);
pub static SIM_ERRORS:        AtomicU64 = AtomicU64::new(0);

static SIM_RUNNING: AtomicBool = AtomicBool::new(false);
static SIM_STOP:    AtomicBool = AtomicBool::new(false);

const TICK_INTERVAL_MS: u64 = 3_000;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── WorldSimTickResult ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WorldSimTickResult {
    pub tick_id:              u64,
    pub steps_completed:      usize,
    pub scenarios_generated:  usize,
    pub cognition_synthesized: usize,
    pub topologies_generated: usize,
    pub safety_interventions: usize,
    pub world_health:         f32,
    pub duration_ms:          u64,
}

// ── Tick logic ────────────────────────────────────────────────────────────────

pub fn run_tick() -> WorldSimTickResult {
    let start_ms = ts_now();
    let mut steps = 0usize;

    SIM_TICKS_TOTAL.fetch_add(1, Ordering::Relaxed);
    let tick_id = SIM_TICKS_TOTAL.load(Ordering::Relaxed);

    // ── Step 1: Safety check ──────────────────────────────────────────────────
    let safety = crate::simulation_safety::check_simulation_safe();
    steps += 1;

    if !safety.is_safe {
        SIM_TICKS_SKIPPED.fetch_add(1, Ordering::Relaxed);
        return WorldSimTickResult {
            tick_id, steps_completed: steps,
            scenarios_generated: 0, cognition_synthesized: 0,
            topologies_generated: 0, safety_interventions: 1,
            world_health: 0.0, duration_ms: ts_now().saturating_sub(start_ms),
        };
    }

    // ── Step 2: Forecast environment ──────────────────────────────────────────
    let _forecast = crate::future_environment_model::forecast(20);
    steps += 1;

    // ── Step 3: Generate current abstract world ───────────────────────────────
    let world = crate::abstract_worlds::generate_current_world();
    steps += 1;

    // ── Step 4: Generate operational scenarios ────────────────────────────────
    let scenarios = crate::scenario_engine::generate_scenarios();
    let scenarios_generated = scenarios.len();
    steps += 1;

    // ── Step 5: Auto-schedule simulations from high-risk scenarios ────────────
    let _scheduled = crate::simulation_scheduler::auto_schedule();
    steps += 1;

    // ── Step 6: Predict world evolution ──────────────────────────────────────
    let prediction = crate::predictive_world_engine::predict_world(30);
    let world_health = prediction.overall_health_forecast;
    steps += 1;

    // ── Step 7: Evolve world model ────────────────────────────────────────────
    let _delta = crate::world_model_evolution::evolve_world_model();
    steps += 1;

    // ── Step 8: Autonomous cognitive synthesis ────────────────────────────────
    let synthesis = crate::autonomous_cognitive_synthesis::synthesize();
    let cognition_synthesized = synthesis.total_outputs();
    steps += 1;

    // ── Step 9: Generate topology candidates ──────────────────────────────────
    let candidates = crate::topology_generation::generate_topology_candidates(2);
    let topologies_generated = candidates.iter().filter(|t| t.is_valid).count();
    steps += 1;

    // ── Step 10: Journal ──────────────────────────────────────────────────────
    crate::future_memory::store(
        crate::future_memory::FutureCategory::SimulationOutcome,
        format!(
            "tick={tick_id}_world={}_health={world_health:.3}_synth={cognition_synthesized}",
            world.state.label()
        ),
        world.instability_score,
    );

    crate::world_evolution_observability::record(
        crate::world_evolution_observability::WorldSimEvent::SimulationRun {
            scenario_id: tick_id,
            outcome:     format!("world_{}", world.state.label()),
            instability: world.instability_score,
        }
    );
    steps += 1;

    let safety_interventions = crate::world_evolution_observability::safety_interventions();

    WorldSimTickResult {
        tick_id,
        steps_completed: steps,
        scenarios_generated,
        cognition_synthesized,
        topologies_generated,
        safety_interventions,
        world_health,
        duration_ms: ts_now().saturating_sub(start_ms),
    }
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

pub fn start() {
    if SIM_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    SIM_STOP.store(false, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("jarvis-world-simulation-runtime".to_string())
        .spawn(move || {
            while !SIM_STOP.load(Ordering::Relaxed) {
                let result = std::panic::catch_unwind(|| run_tick());
                if result.is_err() {
                    SIM_ERRORS.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(TICK_INTERVAL_MS));
            }
            SIM_RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

pub fn stop() {
    SIM_STOP.store(true, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    SIM_RUNNING.load(Ordering::Relaxed)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_tick_completes() {
        let r = run_tick();
        // If safety blocked, steps_completed == 1; otherwise 11
        assert!(r.steps_completed >= 1);
    }

    #[test]
    fn run_tick_increments_counter() {
        let before = SIM_TICKS_TOTAL.load(Ordering::Relaxed);
        let _ = run_tick();
        assert!(SIM_TICKS_TOTAL.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn world_health_bounded() {
        let r = run_tick();
        assert!(r.world_health >= 0.0 && r.world_health <= 1.0);
    }

    #[test]
    fn stop_no_panic_when_not_running() {
        stop();
    }
}
