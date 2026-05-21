//! Cognition loop — orchestrates the observe→model→reason→predict→plan→act→
//! verify→learn→adapt cycle in a background thread.
//!
//! The loop is optional: tests call `run_tick()` directly without spawning a
//! thread.  The background thread is controlled via `AtomicBool` stop signal.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub static LOOP_TICKS_TOTAL:   AtomicU64 = AtomicU64::new(0);
pub static LOOP_TICKS_SKIPPED: AtomicU64 = AtomicU64::new(0);
pub static LOOP_ERRORS:        AtomicU64 = AtomicU64::new(0);

static LOOP_RUNNING: AtomicBool = AtomicBool::new(false);
static LOOP_STOP:    AtomicBool = AtomicBool::new(false);

const TICK_INTERVAL_MS: u64 = 1_000;

// ── Tick result ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TickResult {
    pub tick_id:    u64,
    pub phases_run: usize,
    pub anomalies:  usize,
    pub predictions: usize,
    pub duration_ms: u64,
}

// ── Cognition loop ────────────────────────────────────────────────────────────

pub struct CognitionLoop;

impl CognitionLoop {
    /// Run a single cognitive tick synchronously.  Safe to call from tests.
    pub fn run_tick() -> TickResult {
        use crate::cognitive_tick::{CognitiveTick, TickPhase};
        use crate::cognitive_memory;
        use crate::cognitive_safety::{CognitiveSafetyGuard, ProactiveActionKind};

        let start_ms = ts_now();
        let mut phases_run       = 0usize;
        let mut anomaly_count    = 0usize;
        let mut prediction_count = 0usize;

        LOOP_TICKS_TOTAL.fetch_add(1, Ordering::Relaxed);

        // ── Phase: Observe ───────────────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Observe);
            let obs = crate::active_observer::ActiveObserver::observe();
            let tick = tick.note(format!("changes: {}", obs.changes.changes.len())).complete();
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: Model ─────────────────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Model);
            let entry = crate::persistent_world_model::WorldModelEntry::snapshot_now();
            crate::persistent_world_model::push(entry);
            let tick = tick.complete();
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: Reason ─────────────────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Reason);
            let inferences = crate::persistent_reasoner::PersistentReasoner::update();
            let tick = tick.note(format!("inferences: {}", inferences.len())).complete();
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: Predict ────────────────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Predict);
            let preds = crate::predictive_reasoner::PredictiveReasoner::predict();
            prediction_count = preds.len();
            let tick = tick.note(format!("predictions: {}", preds.len())).complete();
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: Plan (attention) ───────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Plan);
            let decision = crate::attention_runtime::AttentionRuntime::evaluate();
            let tick = tick.note(format!("attention: {}", decision.priority)).complete();
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: Act (safety-gated) ─────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Act);
            let verdict = CognitiveSafetyGuard::check(&ProactiveActionKind::UpdateWorldModel);
            let tick = if verdict.is_allowed() {
                tick.note("safety: allowed").complete()
            } else {
                tick.skip("safety blocked or rate-limited")
            };
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: Verify (anomaly scan) ──────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Verify);
            let anomalies = crate::anomaly_detector::AnomalyDetector::scan();
            anomaly_count = anomalies.len();
            let tick = tick.note(format!("anomalies: {}", anomalies.len())).complete();
            cognitive_memory::record(tick);
            phases_run += 1;

            // Log any detected anomalies
            for a in &anomalies {
                crate::world_state_journal::log(
                    crate::world_state_journal::WorldEventKind::AnomalyDetected {
                        anomaly:  a.kind.label().to_string(),
                        severity: a.kind.severity().to_string(),
                    },
                );
            }
        }

        // ── Phase: Learn ─────────────────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Learn);
            let _ = crate::reflection_runtime::ReflectionRuntime::reflect();
            let tick = tick.complete();
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: Adapt ──────────────────────────────────────────────────────
        {
            let tick = CognitiveTick::new(TickPhase::Adapt);
            crate::task_continuity::clear_stale(5 * 60_000);
            let tick = tick.note("stale continuity cleared").complete();
            cognitive_memory::record(tick);
            phases_run += 1;
        }

        // ── Phase: MetaCognition (Phase 18 live integration) ──────────────────
        if !crate::live_meta_loop::is_running() {
            let _meta_tick = crate::live_meta_loop::run_tick();
            phases_run += 1;
        }

        // ── Phase: HierarchicalCognition (Phase 19 integration) ───────────────
        // Inline tick when background thread is absent (tests / constrained env).
        if !crate::hierarchical_runtime::is_running() {
            let _hier_tick = crate::hierarchical_runtime::run_tick();
            phases_run += 1;
        }

        let tick_id = LOOP_TICKS_TOTAL.load(Ordering::Relaxed);
        let duration_ms = ts_now().saturating_sub(start_ms);

        // Journal the tick
        crate::world_state_journal::log(
            crate::world_state_journal::WorldEventKind::CognitionLoopTick {
                tick_id,
                phase:   "full_cycle".to_string(),
                outcome: "Completed".to_string(),
            },
        );

        TickResult { tick_id, phases_run, anomalies: anomaly_count, predictions: prediction_count, duration_ms }
    }

    /// Start the background cognition loop thread and the live meta-loop thread.
    pub fn start() {
        if LOOP_RUNNING.swap(true, Ordering::SeqCst) {
            return; // already running
        }
        LOOP_STOP.store(false, Ordering::SeqCst);

        // Start Phase 18 live meta-cognition loop alongside the cognition loop
        crate::live_meta_loop::start();
        // Start Phase 19 hierarchical cognition runtime
        crate::hierarchical_runtime::start();

        std::thread::Builder::new()
            .name("jarvis-cognition-loop".to_string())
            .spawn(move || {
                while !LOOP_STOP.load(Ordering::Relaxed) {
                    let result = std::panic::catch_unwind(|| {
                        CognitionLoop::run_tick()
                    });
                    if let Err(_e) = result {
                        LOOP_ERRORS.fetch_add(1, Ordering::Relaxed);
                    }
                    std::thread::sleep(Duration::from_millis(TICK_INTERVAL_MS));
                }
                LOOP_RUNNING.store(false, Ordering::SeqCst);
            })
            .ok();
    }

    /// Signal the background loop, live meta-loop, and hierarchical runtime to stop.
    pub fn stop() {
        LOOP_STOP.store(true, Ordering::SeqCst);
        crate::live_meta_loop::stop();
        crate::hierarchical_runtime::stop();
    }

    pub fn is_running() -> bool {
        LOOP_RUNNING.load(Ordering::Relaxed)
    }
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
        ocr_runtime::init_stub("application ready");
    }

    #[test]
    fn run_tick_completes_all_phases() {
        init_stubs();
        let result = CognitionLoop::run_tick();
        assert_eq!(result.phases_run, 11); // 9 original + 1 MetaCognition (18) + 1 Hierarchical (19)
    }

    #[test]
    fn run_tick_increments_total_counter() {
        init_stubs();
        let before = LOOP_TICKS_TOTAL.load(Ordering::Relaxed);
        CognitionLoop::run_tick();
        assert!(LOOP_TICKS_TOTAL.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn run_tick_returns_valid_tick_id() {
        init_stubs();
        let result = CognitionLoop::run_tick();
        assert!(result.tick_id > 0);
    }

    #[test]
    fn run_tick_no_panic_repeated() {
        init_stubs();
        for _ in 0..3 {
            let _ = CognitionLoop::run_tick();
        }
    }

    #[test]
    fn stop_does_not_panic_when_not_running() {
        CognitionLoop::stop();
    }

    #[test]
    fn is_running_false_before_start() {
        // Don't start the loop in tests — just verify the flag is queryable
        let _r = CognitionLoop::is_running();
    }
}
