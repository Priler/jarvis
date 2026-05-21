//! Live meta-cognition loop — Phase 18 central runtime integration.
//! Runs continuously alongside the cognition loop, coordinating all
//! meta-cognitive subsystems via the meta_scheduler and cognitive_watchdog.
//!
//! This is the integration point that makes meta-cognition truly live:
//! every tick it evaluates whether each subsystem is due, runs it if so,
//! collects events, and feeds the observability pipeline.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub static META_LOOP_TICKS:    AtomicU64 = AtomicU64::new(0);
pub static META_LOOP_ERRORS:   AtomicU64 = AtomicU64::new(0);
pub static META_LOOP_RUNNING:  AtomicBool = AtomicBool::new(false);

static META_LOOP_STOP: AtomicBool = AtomicBool::new(false);

const TICK_MS: u64 = 1_000;   // 1-second meta loop tick

// ── Live meta tick result ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MetaLoopTick {
    pub tick_id:           u64,
    pub subsystems_run:    Vec<&'static str>,
    pub events_processed:  usize,
    pub watchdog_frozen:   bool,
    pub duration_ms:       u64,
}

// ── Core tick ─────────────────────────────────────────────────────────────────

/// Execute one meta-loop tick synchronously.  Safe to call from tests.
pub fn run_tick() -> MetaLoopTick {
    use crate::meta_scheduler::{is_due, Subsystem};
    use crate::cognitive_watchdog;
    use crate::meta_event_bus;
    use crate::live_meta_observability as obs;

    let start = ts_now();
    META_LOOP_TICKS.fetch_add(1, Ordering::Relaxed);
    let tick_id = META_LOOP_TICKS.load(Ordering::Relaxed);
    let mut subsystems_run: Vec<&'static str> = Vec::new();

    // ── 1. Watchdog (always first, drives freeze state) ───────────────────────
    if is_due(Subsystem::Watchdog) {
        let report = cognitive_watchdog::check();
        subsystems_run.push("watchdog");
        if report.any_intervention() {
            obs::log(
                obs::ObsCategory::WatchdogEvent,
                format!("interventions: {:?}", report.interventions),
                obs::ObsSeverity::Warning,
            );
        }
    }

    // If cognition is frozen by watchdog, skip all non-safety subsystems
    let frozen = cognitive_watchdog::is_frozen();

    // ── 2. Meta-cognition cycle ───────────────────────────────────────────────
    if !frozen && is_due(Subsystem::MetaCycle) {
        let result = crate::meta_cognition_runtime::run_cycle();
        subsystems_run.push("meta_cycle");

        // Feed result into watchdog for storm/oscillation tracking
        let sim_passed = if crate::cognitive_watchdog::sims_suppressed() { None } else { Some(true) };
        cognitive_watchdog::record_meta_cycle(
            result.strategy_changed,
            sim_passed,
            result.confidence,
        );

        // Publish completion event
        meta_event_bus::publish(meta_event_bus::MetaEvent::MetaCycleCompleted {
            cycle_id: result.cycle_id,
            healthy:  result.is_healthy(),
        });

        if !result.is_healthy() {
            meta_event_bus::publish(meta_event_bus::MetaEvent::ReasoningFailure {
                quality: result.reasoning_quality,
                cycle:   result.cycle_id,
            });
        }

        obs::log(
            obs::ObsCategory::MetaCycle,
            format!("meta_cycle id={} healthy={} unc={:.3}",
                result.cycle_id, result.is_healthy(), result.overall_uncertainty),
            if result.is_healthy() { obs::ObsSeverity::Info } else { obs::ObsSeverity::Warning },
        );
    }

    // ── 3. Live uncertainty calibration ──────────────────────────────────────
    if !frozen && is_due(Subsystem::Uncertainty) {
        let snap = crate::live_uncertainty::calibrate();
        subsystems_run.push("uncertainty");
        obs::log(
            obs::ObsCategory::UncertaintyRecalib,
            format!("calibration overall={:.3} critical={}", snap.overall, snap.critical_count),
            if snap.critical_count > 0 { obs::ObsSeverity::Warning } else { obs::ObsSeverity::Info },
        );
    }

    // ── 4. Causal analysis ────────────────────────────────────────────────────
    if !frozen && is_due(Subsystem::CausalAnalysis) {
        let links = crate::causal_reasoner::reliable_links();
        subsystems_run.push("causal_analysis");

        // Observe causal relationships from current runtime signals
        let unc = crate::uncertainty_engine::sample();
        if unc.overall > 0.6 {
            crate::causal_reasoner::observe("high_uncertainty", "reasoning_degradation", unc.overall);
        }
        let stability = crate::cognitive_stability::check();
        if stability.is_unstable() {
            crate::causal_reasoner::observe("cognitive_instability", "planner_degradation",
                stability.oscillation_score);
        }

        // Check for instability in known links
        for link in links.iter().filter(|l| l.strength < 0.35) {
            meta_event_bus::publish(meta_event_bus::MetaEvent::CausalInstability {
                cause:        link.cause.clone(),
                effect:       link.effect.clone(),
                strength_drop: 0.35 - link.strength,
            });
        }
        obs::log(
            obs::ObsCategory::CausalUpdate,
            format!("causal links={} reliable={}", links.len(), links.iter().filter(|l| l.is_reliable()).count()),
            obs::ObsSeverity::Info,
        );
    }

    // ── 5. Strategy simulation ────────────────────────────────────────────────
    if !frozen && !crate::cognitive_watchdog::sims_suppressed() && is_due(Subsystem::Simulation) {
        let plan = crate::strategy_simulator::Plan {
            id:    format!("live_sim_{}", tick_id),
            steps: vec![
                crate::strategy_simulator::PlanStep {
                    tool_id:         "observe".to_string(),
                    estimated_risk:  0.1,
                    requires_verify: false,
                },
                crate::strategy_simulator::PlanStep {
                    tool_id:         "plan".to_string(),
                    estimated_risk:  0.2,
                    requires_verify: true,
                },
            ],
        };
        let sim_result = crate::strategy_simulator::simulate(&plan);
        subsystems_run.push("simulation");

        cognitive_watchdog::record_meta_cycle(false, Some(sim_result.should_execute), 0.7);

        meta_event_bus::publish(meta_event_bus::MetaEvent::SimulationResult {
            plan_id:       sim_result.plan_id.clone(),
            success_prob:  sim_result.success_prob,
            should_execute: sim_result.should_execute,
        });

        obs::log(
            obs::ObsCategory::Simulation,
            format!("sim plan={} prob={:.3} safe={}",
                sim_result.plan_id, sim_result.success_prob, sim_result.is_safe()),
            obs::ObsSeverity::Info,
        );
    }

    // ── 6. Strategic arbitration ──────────────────────────────────────────────
    if !frozen && is_due(Subsystem::Arbitration) {
        let result = crate::live_strategy_arbitration::run_live();
        subsystems_run.push("arbitration");
        obs::log(
            obs::ObsCategory::Arbitration,
            format!("arbitration verdict={:?}", std::mem::discriminant(&result.verdict)),
            obs::ObsSeverity::Info,
        );
    }

    // ── 7. Meta-reflection ────────────────────────────────────────────────────
    if !frozen && is_due(Subsystem::Reflection) {
        let report = crate::meta_reflection::reflect();
        subsystems_run.push("reflection");

        for insight in report.insights.iter().filter(|i| i.is_failure) {
            meta_event_bus::publish(meta_event_bus::MetaEvent::ReflectionEvent {
                insight:    insight.insight.clone(),
                severity:   insight.severity,
                is_failure: true,
            });
        }
        obs::log(
            obs::ObsCategory::MetaCycle,
            format!("reflection health={:.3} failures={}", report.overall_health, report.failure_count),
            if report.has_critical_failures() { obs::ObsSeverity::Critical } else { obs::ObsSeverity::Info },
        );
    }

    // ── 8. Counterfactual evaluation ──────────────────────────────────────────
    if !frozen && is_due(Subsystem::Counterfactual) {
        let cf = crate::live_counterfactuals::evaluate();
        subsystems_run.push("counterfactual");
        obs::log(
            obs::ObsCategory::Counterfactual,
            format!("cf baseline={:.3} best_gain={:.3} rec={}",
                cf.baseline_quality, cf.best_improvement, cf.recommendation),
            obs::ObsSeverity::Info,
        );
    }

    // ── 9. Memory fusion ──────────────────────────────────────────────────────
    if !frozen && is_due(Subsystem::MemoryFusion) {
        let fusion = crate::meta_memory_fusion::fuse();
        subsystems_run.push("memory_fusion");
        obs::log(
            obs::ObsCategory::MemoryFusion,
            format!("fusion entries={} coherence={:.3} conflicts={}",
                fusion.entries_merged, fusion.overall_coherence, fusion.conflicts),
            if fusion.is_coherent() { obs::ObsSeverity::Info } else { obs::ObsSeverity::Warning },
        );
    }

    // ── 10. Drain + process event bus ─────────────────────────────────────────
    let events = meta_event_bus::drain();
    let events_processed = events.len();
    obs::process_bus_events(&events);

    let duration_ms = ts_now().saturating_sub(start);

    MetaLoopTick {
        tick_id,
        subsystems_run,
        events_processed,
        watchdog_frozen: frozen,
        duration_ms,
    }
}

// ── Background thread ─────────────────────────────────────────────────────────

/// Start the live meta-loop background thread.
pub fn start() {
    if META_LOOP_RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    META_LOOP_STOP.store(false, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("jarvis-meta-loop".to_string())
        .spawn(move || {
            while !META_LOOP_STOP.load(Ordering::Relaxed) {
                let result = std::panic::catch_unwind(|| run_tick());
                if let Err(_e) = result {
                    META_LOOP_ERRORS.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(TICK_MS));
            }
            META_LOOP_RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

/// Stop the live meta-loop background thread.
pub fn stop() {
    META_LOOP_STOP.store(true, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    META_LOOP_RUNNING.load(Ordering::Relaxed)
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
    use crate::screen_capture;
    use crate::ocr_runtime;

    fn init_stubs() {
        screen_capture::init_stub();
        ocr_runtime::init_stub("meta loop ready");
    }

    #[test]
    fn run_tick_completes() {
        init_stubs();
        let tick = run_tick();
        assert!(tick.tick_id >= 1);
        assert!(tick.duration_ms < 5_000);
    }

    #[test]
    fn multiple_ticks_increment_counter() {
        init_stubs();
        let before = META_LOOP_TICKS.load(Ordering::Relaxed);
        run_tick();
        run_tick();
        assert!(META_LOOP_TICKS.load(Ordering::Relaxed) >= before + 2);
    }

    #[test]
    fn watchdog_gate_does_not_panic_when_frozen() {
        // Force frozen state and verify tick survives
        crate::cognitive_watchdog::COGNITION_FROZEN.store(true, Ordering::SeqCst);
        init_stubs();
        let tick = run_tick();
        assert!(tick.watchdog_frozen);
        crate::cognitive_watchdog::COGNITION_FROZEN.store(false, Ordering::SeqCst);
    }
}
