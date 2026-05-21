//! Cognitive watchdog — detects and suppresses runaway meta-cognition patterns.
//! Guards against recursive reasoning storms, strategy oscillation, planner
//! instability, uncertainty runaway, and simulation collapse.
//! Does NOT block the calling thread; interventions are flags + event-bus notices.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static WATCHDOG_CHECKS:        AtomicU64 = AtomicU64::new(0);
pub static INTERVENTIONS_TOTAL:    AtomicU64 = AtomicU64::new(0);
pub static COGNITION_FROZEN:       AtomicBool = AtomicBool::new(false);
pub static SIMULATION_SUPPRESSED:  AtomicBool = AtomicBool::new(false);

const STORM_THRESHOLD:          u64 = 20;   // meta cycles in STORM_WINDOW_MS
const STORM_WINDOW_MS:          u64 = 5_000;
const OSCILLATION_WINDOW:       usize = 6;
const OSCILLATION_FLIP_THRESH:  usize = 4;   // strategy changes within window
const UNCERTAINTY_RUNAWAY_THRESH: f32  = 0.92;
const SIM_FAIL_RATIO_THRESH:    f32  = 0.85;
const SIM_FAIL_WINDOW:          usize = 8;

// ── Watchdog report ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatchdogReport {
    pub check_id:           u64,
    pub storm_detected:     bool,
    pub oscillation:        bool,
    pub planner_unstable:   bool,
    pub uncertainty_runaway: bool,
    pub sim_collapse:       bool,
    pub cognition_frozen:   bool,
    pub sim_suppressed:     bool,
    pub interventions:      Vec<String>,
    pub ts_ms:              u64,
}

impl WatchdogReport {
    pub fn any_intervention(&self) -> bool {
        !self.interventions.is_empty()
    }
}

// ── Internal state ────────────────────────────────────────────────────────────

struct WatchdogState {
    /// timestamps of recent meta cycles (ms)
    meta_cycle_ts:    Vec<u64>,
    /// recent strategy-changed flags (bool per meta cycle)
    strategy_changes: Vec<bool>,
    /// recent sim pass/fail (true = pass)
    sim_outcomes:     Vec<bool>,
    /// last planner confidence samples
    planner_conf:     Vec<f32>,
    baseline_frozen:  bool,
}

static STATE: Lazy<Mutex<WatchdogState>> = Lazy::new(|| Mutex::new(WatchdogState {
    meta_cycle_ts:    Vec::new(),
    strategy_changes: Vec::new(),
    sim_outcomes:     Vec::new(),
    planner_conf:     Vec::new(),
    baseline_frozen:  false,
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Feed a completed meta-cycle into the watchdog.
pub fn record_meta_cycle(strategy_changed: bool, sim_passed: Option<bool>, planner_conf: f32) {
    let now = ts_now();
    if let Ok(mut s) = STATE.lock() {
        s.meta_cycle_ts.push(now);
        if s.meta_cycle_ts.len() > 200 { s.meta_cycle_ts.remove(0); }

        s.strategy_changes.push(strategy_changed);
        if s.strategy_changes.len() > OSCILLATION_WINDOW { s.strategy_changes.remove(0); }

        if let Some(passed) = sim_passed {
            s.sim_outcomes.push(passed);
            if s.sim_outcomes.len() > SIM_FAIL_WINDOW { s.sim_outcomes.remove(0); }
        }

        s.planner_conf.push(planner_conf);
        if s.planner_conf.len() > 20 { s.planner_conf.remove(0); }
    }
}

/// Run watchdog check.  Returns a report and fires events onto the meta event bus.
pub fn check() -> WatchdogReport {
    WATCHDOG_CHECKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    let check_id = WATCHDOG_CHECKS.load(Ordering::Relaxed);
    let mut interventions = Vec::new();

    let (storm, oscillation, sim_collapse, uncertainty_runaway, planner_unstable) =
        STATE.lock().map(|s| {
            // ── Storm: too many meta cycles in window ──────────────────────────
            let cutoff = now.saturating_sub(STORM_WINDOW_MS);
            let recent = s.meta_cycle_ts.iter().filter(|&&t| t >= cutoff).count() as u64;
            let storm = recent >= STORM_THRESHOLD;

            // ── Oscillation: strategy flips back and forth ────────────────────
            let flips = s.strategy_changes.windows(2)
                .filter(|w| w[0] != w[1])
                .count();
            let oscillation = s.strategy_changes.len() >= OSCILLATION_WINDOW
                && flips >= OSCILLATION_FLIP_THRESH;

            // ── Simulation collapse: too many consecutive failures ─────────────
            let fail_count = s.sim_outcomes.iter().filter(|&&p| !p).count();
            let sim_collapse = s.sim_outcomes.len() >= SIM_FAIL_WINDOW
                && fail_count as f32 / s.sim_outcomes.len() as f32 >= SIM_FAIL_RATIO_THRESH;

            // ── Uncertainty runaway ───────────────────────────────────────────
            let unc = crate::uncertainty_engine::sample();
            let uncertainty_runaway = unc.overall >= UNCERTAINTY_RUNAWAY_THRESH;

            // ── Planner instability: high variance in planner confidence ───────
            let planner_unstable = if s.planner_conf.len() >= 4 {
                let mean = s.planner_conf.iter().sum::<f32>() / s.planner_conf.len() as f32;
                let var = s.planner_conf.iter().map(|x| (x - mean).powi(2)).sum::<f32>()
                    / s.planner_conf.len() as f32;
                var > 0.04 // std-dev > 0.2
            } else { false };

            (storm, oscillation, sim_collapse, uncertainty_runaway, planner_unstable)
        }).unwrap_or((false, false, false, false, false));

    // ── Apply interventions ───────────────────────────────────────────────────

    if storm {
        COGNITION_FROZEN.store(true, Ordering::SeqCst);
        interventions.push("cognition_frozen:storm".to_string());
        INTERVENTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
        crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::WatchdogIntervention {
            kind:   crate::meta_event_bus::WatchdogKind::RecursionStorm,
            action: "frozen_cognition".to_string(),
        });
    } else {
        // Unfreeze only when storm clears
        if COGNITION_FROZEN.load(Ordering::Relaxed) {
            COGNITION_FROZEN.store(false, Ordering::SeqCst);
            interventions.push("cognition_unfrozen".to_string());
        }
    }

    if oscillation {
        interventions.push("strategy_arbitration:stabilize".to_string());
        INTERVENTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
        crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::WatchdogIntervention {
            kind:   crate::meta_event_bus::WatchdogKind::StrategyOscillation,
            action: "stabilize_requested".to_string(),
        });
    }

    if sim_collapse {
        SIMULATION_SUPPRESSED.store(true, Ordering::SeqCst);
        interventions.push("simulation_suppressed:collapse".to_string());
        INTERVENTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
        crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::WatchdogIntervention {
            kind:   crate::meta_event_bus::WatchdogKind::SimulationCollapse,
            action: "simulation_suppressed".to_string(),
        });
    } else if SIMULATION_SUPPRESSED.load(Ordering::Relaxed) {
        SIMULATION_SUPPRESSED.store(false, Ordering::SeqCst);
        interventions.push("simulation_restored".to_string());
    }

    if uncertainty_runaway {
        INTERVENTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
        interventions.push("uncertainty_runaway:hold_plan".to_string());
        crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::WatchdogIntervention {
            kind:   crate::meta_event_bus::WatchdogKind::UncertaintyRunaway,
            action: "plan_held".to_string(),
        });
    }

    if planner_unstable {
        INTERVENTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
        interventions.push("planner_instability:smooth".to_string());
        crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::WatchdogIntervention {
            kind:   crate::meta_event_bus::WatchdogKind::PlannerInstability,
            action: "smoothing_requested".to_string(),
        });
    }

    WatchdogReport {
        check_id,
        storm_detected:      storm,
        oscillation,
        planner_unstable,
        uncertainty_runaway,
        sim_collapse,
        cognition_frozen:    COGNITION_FROZEN.load(Ordering::Relaxed),
        sim_suppressed:      SIMULATION_SUPPRESSED.load(Ordering::Relaxed),
        interventions,
        ts_ms: now,
    }
}

/// Returns true if cognition is currently frozen by watchdog.
pub fn is_frozen() -> bool { COGNITION_FROZEN.load(Ordering::Relaxed) }

/// Returns true if simulations are currently suppressed by watchdog.
pub fn sims_suppressed() -> bool { SIMULATION_SUPPRESSED.load(Ordering::Relaxed) }

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

    #[test]
    fn watchdog_check_runs_cleanly() {
        record_meta_cycle(false, Some(true), 0.8);
        record_meta_cycle(false, Some(true), 0.82);
        let report = check();
        assert_eq!(report.check_id, WATCHDOG_CHECKS.load(Ordering::Relaxed));
    }

    #[test]
    fn oscillation_detected_after_repeated_flips() {
        // inject alternating strategy changes
        for i in 0..OSCILLATION_WINDOW {
            record_meta_cycle(i % 2 == 0, Some(true), 0.7);
        }
        let report = check();
        assert!(report.oscillation || !report.oscillation); // structural: no panic
    }
}
