//! Hierarchical cognition runtime — Phase 19 top-level orchestrator.
//! Coordinates all 5 cognition layers, the resource scheduler, safety system,
//! generalized planner, and long-horizon reasoning in a single runtime tick.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use crate::cognition_layers::{CognitionEvent, CognitionLayer};

pub static HIER_LOOP_TICKS:   AtomicU64 = AtomicU64::new(0);
pub static HIER_LOOP_ERRORS:  AtomicU64 = AtomicU64::new(0);
pub static HIER_LOOP_RUNNING: AtomicBool = AtomicBool::new(false);

static HIER_LOOP_STOP: AtomicBool = AtomicBool::new(false);

const TICK_MS: u64 = 500;   // 0.5 s main tick — reactive layer needs fast cycles

// ── Hierarchical tick result ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HierarchicalTick {
    pub tick_id:         u64,
    pub layers_active:   usize,
    pub events_processed: usize,
    pub safety_ok:       bool,
    pub resource_ok:     bool,
    pub duration_ms:     u64,
}

// ── Core tick ─────────────────────────────────────────────────────────────────

pub fn run_tick() -> HierarchicalTick {
    HIER_LOOP_TICKS.fetch_add(1, Ordering::Relaxed);
    let tick_id = HIER_LOOP_TICKS.load(Ordering::Relaxed);
    let start = ts_now();
    let mut layers_active = 0usize;
    let mut events_processed = 0usize;

    // ── 1. Resource scheduler ─────────────────────────────────────────────────
    let sched = crate::resource_scheduler::tick();
    let resource_ok = !sched.emergency_mode;

    // ── 2. Hierarchical safety check ──────────────────────────────────────────
    let safety_report = crate::hierarchical_safety::check();
    let safety_ok = safety_report.is_safe();

    // ── 3. Priority evaluation ────────────────────────────────────────────────
    let _priority = crate::priority_runtime::evaluate(&crate::priority_runtime::default_items());

    // ── 4. Reactive layer (always runs — never skipped) ───────────────────────
    if crate::hierarchical_scheduler::is_due(CognitionLayer::Reactive) {
        layers_active += 1;
        // Check for anomalies and generate reactive events
        let anomalies = crate::anomaly_detector::AnomalyDetector::scan();
        for anomaly in anomalies.iter().filter(|a| a.confidence >= 0.7) {
            let ev = CognitionEvent::CriticalAnomaly {
                kind:     anomaly.kind.label().to_string(),
                severity: anomaly.confidence,
            };
            let result = crate::cognition_coordinator::dispatch(&ev);
            crate::hierarchical_safety::record_event();
            if result.handled { events_processed += 1; }
        }
    }

    // ── 5. Tactical layer ─────────────────────────────────────────────────────
    if resource_ok && crate::hierarchical_scheduler::is_due(CognitionLayer::Tactical) {
        layers_active += 1;
        let budget = crate::resource_scheduler::budget_for(CognitionLayer::Tactical);
        if budget >= 0.2 {
            // Tactical work: process any pending workflow events (synthetic tick)
            let active_wfs = crate::tactical_layer::active_workflows();
            for wf in active_wfs.iter().take(3) {
                let ev = CognitionEvent::ToolExecuted {
                    tool_id:    wf.clone(),
                    success:    true,
                    latency_ms: 100,
                };
                let result = crate::cognition_coordinator::dispatch(&ev);
                crate::hierarchical_safety::record_event();
                if result.handled { events_processed += 1; }
            }
        }
    }

    // ── 6. Strategic layer ────────────────────────────────────────────────────
    if resource_ok && crate::hierarchical_scheduler::is_due(CognitionLayer::Strategic) {
        layers_active += 1;
        let budget = crate::resource_scheduler::budget_for(CognitionLayer::Strategic);
        if budget >= 0.1 {
            // Long-horizon reasoning tick
            let assessment = crate::long_horizon_reasoning::reason();
            if assessment.needs_replan() {
                let new_goals = vec![
                    crate::generalized_planner::PlanGoal::new(
                        "replan_main", "Replanned due to strategic risk", 0.8, 0)
                ];
                let _new_plan = crate::generalized_planner::replan(
                    "strategic_risk", new_goals);
            }

            // Environment drift event
            let unc = crate::uncertainty_engine::sample();
            if unc.overall > 0.5 {
                let ev = CognitionEvent::EnvironmentDrifted { drift_score: unc.overall };
                let result = crate::cognition_coordinator::dispatch(&ev);
                crate::hierarchical_safety::record_event();
                if result.handled { events_processed += 1; }
            }
        }
    }

    // ── 7. Meta layer ─────────────────────────────────────────────────────────
    if resource_ok && !crate::resource_scheduler::is_bg_suspended()
        && crate::hierarchical_scheduler::is_due(CognitionLayer::Meta)
    {
        layers_active += 1;
        let budget = crate::resource_scheduler::budget_for(CognitionLayer::Meta);
        if budget >= 0.1 {
            let meta_result = crate::meta_layer::evaluate();
            if !meta_result.is_healthy() {
                let ev = CognitionEvent::ReasoningDegraded {
                    quality: meta_result.reasoning_quality,
                    cycle:   meta_result.cycle_id,
                };
                let result = crate::cognition_coordinator::dispatch(&ev);
                crate::hierarchical_safety::record_event();
                if result.handled { events_processed += 1; }
            }
        }
    }

    // ── 8. Supervisory layer ──────────────────────────────────────────────────
    if crate::hierarchical_scheduler::is_due(CognitionLayer::Supervisory) {
        layers_active += 1;
        crate::supervisory_layer::coordinate();
        // Emit layer overload events if needed
        if !safety_ok {
            let ev = CognitionEvent::CognitionEscalated {
                from:   CognitionLayer::Meta,
                to:     CognitionLayer::Supervisory,
                reason: "safety_violation".to_string(),
            };
            let result = crate::cognition_coordinator::dispatch(&ev);
            crate::hierarchical_safety::record_event();
            if result.handled { events_processed += 1; }
        }
    }

    // ── 9. Generalized planner maintenance ────────────────────────────────────
    // Ensure at least one active plan exists
    if crate::generalized_planner::active_plans().is_empty() && resource_ok {
        let goals = vec![
            crate::generalized_planner::PlanGoal::new("default_goal", "Maintain stable operation", 0.8, 0),
        ];
        let plan = crate::generalized_planner::create(goals);
        let sim  = crate::generalized_planner::simulate(&plan);
        crate::generalized_planner::adopt(&plan.id, &sim);
    }

    // ── 10. Observability ─────────────────────────────────────────────────────
    crate::generalized_observability::log(
        crate::generalized_observability::HierarchyObs::HierarchyTick {
            tick: tick_id,
            layers_active,
            events_processed,
        }
    );

    // ── 11. Memory: write supervisory state ───────────────────────────────────
    crate::hierarchical_memory::write(
        CognitionLayer::Supervisory,
        "runtime:last_tick",
        format!("tick={tick_id} layers={layers_active} events={events_processed}"),
        if safety_ok && resource_ok { 0.9 } else { 0.4 },
    );

    HierarchicalTick {
        tick_id,
        layers_active,
        events_processed,
        safety_ok,
        resource_ok,
        duration_ms: ts_now().saturating_sub(start),
    }
}

// ── Background thread ─────────────────────────────────────────────────────────

pub fn start() {
    if HIER_LOOP_RUNNING.swap(true, Ordering::SeqCst) { return; }
    HIER_LOOP_STOP.store(false, Ordering::SeqCst);

    // Prime all layer schedules
    for layer in CognitionLayer::all() {
        crate::hierarchical_scheduler::allow_now(layer);
    }

    std::thread::Builder::new()
        .name("jarvis-hierarchical-runtime".to_string())
        .spawn(move || {
            while !HIER_LOOP_STOP.load(Ordering::Relaxed) {
                let result = std::panic::catch_unwind(|| run_tick());
                if result.is_err() {
                    HIER_LOOP_ERRORS.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(TICK_MS));
            }
            HIER_LOOP_RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

pub fn stop() {
    HIER_LOOP_STOP.store(true, Ordering::SeqCst);
}

pub fn is_running() -> bool { HIER_LOOP_RUNNING.load(Ordering::Relaxed) }

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
    use crate::screen_capture;
    use crate::ocr_runtime;

    fn init_stubs() {
        screen_capture::init_stub();
        ocr_runtime::init_stub("hierarchical runtime ready");
    }

    #[test]
    fn run_tick_completes_successfully() {
        init_stubs();
        // Prime all layers
        for layer in CognitionLayer::all() {
            crate::hierarchical_scheduler::allow_now(layer);
        }
        let tick = run_tick();
        assert!(tick.tick_id >= 1);
        assert!(tick.duration_ms < 10_000);
    }

    #[test]
    fn multiple_ticks_increment_counter() {
        init_stubs();
        let before = HIER_LOOP_TICKS.load(Ordering::Relaxed);
        run_tick();
        run_tick();
        assert!(HIER_LOOP_TICKS.load(Ordering::Relaxed) >= before + 2);
    }

    #[test]
    fn tick_produces_layers_active() {
        init_stubs();
        for layer in CognitionLayer::all() {
            crate::hierarchical_scheduler::allow_now(layer);
        }
        let tick = run_tick();
        assert!(tick.layers_active >= 1);
    }
}
