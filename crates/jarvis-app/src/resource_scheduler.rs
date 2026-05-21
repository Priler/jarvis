//! Cognitive resource scheduler — balances cognition work across layers,
//! throttles simulations, schedules background cognition, and suppresses
//! non-critical work under emergency conditions.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::CognitionLayer;

pub static SCHEDULER_TICKS:      AtomicU64 = AtomicU64::new(0);
pub static THROTTLE_ACTIVATIONS: AtomicU64 = AtomicU64::new(0);
pub static EMERGENCY_SUPPRESSED: AtomicBool = AtomicBool::new(false);
pub static BACKGROUND_SUSPENDED: AtomicBool = AtomicBool::new(false);

// Layer work budgets (relative, 0–1)
// Reactive always gets full budget; others scale down under load
const REACTIVE_BUDGET:    f32 = 1.0;
const TACTICAL_BUDGET:    f32 = 0.8;
const STRATEGIC_BUDGET:   f32 = 0.5;
const META_BUDGET:        f32 = 0.4;
const SUPERVISORY_BUDGET: f32 = 0.3;

const THROTTLE_THRESHOLD:   f32 = 0.75;
const EMERGENCY_THRESHOLD:  f32 = 0.90;
const MAX_TICK_HISTORY:   usize = 80;

// ── Scheduler tick result ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SchedulerTickResult {
    pub tick:              u64,
    pub layer_budgets:     Vec<(String, f32)>,   // (layer label, budget)
    pub throttled:         bool,
    pub emergency_mode:    bool,
    pub bg_suspended:      bool,
    pub ts_ms:             u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SchedState {
    history: Vec<SchedulerTickResult>,
}

static STATE: Lazy<Mutex<SchedState>> = Lazy::new(|| Mutex::new(SchedState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Run one scheduler tick.  Adjusts layer budgets based on resource state.
pub fn tick() -> SchedulerTickResult {
    SCHEDULER_TICKS.fetch_add(1, Ordering::Relaxed);
    let tick_id = SCHEDULER_TICKS.load(Ordering::Relaxed);
    let now = ts_now();

    let res = crate::resource_reasoner::sample();
    let overall = res.overall;

    let throttled  = overall >= THROTTLE_THRESHOLD;
    let emergency  = overall >= EMERGENCY_THRESHOLD;

    if emergency {
        EMERGENCY_SUPPRESSED.store(true, Ordering::SeqCst);
        BACKGROUND_SUSPENDED.store(true, Ordering::SeqCst);
        THROTTLE_ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
        // Suppress expensive meta subsystems
        crate::meta_scheduler::suppress(crate::meta_scheduler::Subsystem::Simulation);
        crate::meta_scheduler::suppress(crate::meta_scheduler::Subsystem::Counterfactual);
        crate::meta_scheduler::suppress(crate::meta_scheduler::Subsystem::MemoryFusion);
    } else if throttled {
        THROTTLE_ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
        BACKGROUND_SUSPENDED.store(true, Ordering::SeqCst);
        if EMERGENCY_SUPPRESSED.load(Ordering::Relaxed) {
            // Recover from emergency
            EMERGENCY_SUPPRESSED.store(false, Ordering::SeqCst);
            crate::meta_scheduler::allow_now(crate::meta_scheduler::Subsystem::Simulation);
        }
    } else {
        // Normal operation — restore background work
        BACKGROUND_SUSPENDED.store(false, Ordering::SeqCst);
        if EMERGENCY_SUPPRESSED.load(Ordering::Relaxed) {
            EMERGENCY_SUPPRESSED.store(false, Ordering::SeqCst);
        }
    }

    // Compute per-layer budgets (reactive never reduced)
    let scale = if emergency { 0.3 } else if throttled { 0.6 } else { 1.0 };

    let layer_budgets: Vec<(String, f32)> = vec![
        (CognitionLayer::Reactive.label().to_string(),    REACTIVE_BUDGET),   // never reduced
        (CognitionLayer::Tactical.label().to_string(),    (TACTICAL_BUDGET    * scale).clamp(0.1, 1.0)),
        (CognitionLayer::Strategic.label().to_string(),   (STRATEGIC_BUDGET   * scale).clamp(0.05, 1.0)),
        (CognitionLayer::Meta.label().to_string(),        (META_BUDGET        * scale).clamp(0.05, 1.0)),
        (CognitionLayer::Supervisory.label().to_string(), (SUPERVISORY_BUDGET * scale).clamp(0.05, 1.0)),
    ];

    // Emit observability
    if throttled || emergency {
        crate::generalized_observability::log(
            crate::generalized_observability::HierarchyObs::SchedulerIntervention {
                subsystem: "resource_scheduler".to_string(),
                action:    if emergency { "emergency_suppress" } else { "throttle" }.to_string(),
            }
        );
    }

    let result = SchedulerTickResult {
        tick: tick_id, layer_budgets, throttled,
        emergency_mode: emergency, bg_suspended: BACKGROUND_SUSPENDED.load(Ordering::Relaxed),
        ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_TICK_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

/// Returns the current budget (0–1) for a given layer.
pub fn budget_for(layer: CognitionLayer) -> f32 {
    if layer == CognitionLayer::Reactive { return 1.0; }
    if EMERGENCY_SUPPRESSED.load(Ordering::Relaxed) {
        if layer == CognitionLayer::Tactical { return 0.3; }
        return 0.05;
    }
    if BACKGROUND_SUSPENDED.load(Ordering::Relaxed) {
        match layer {
            CognitionLayer::Tactical    => 0.6,
            CognitionLayer::Strategic   => 0.3,
            CognitionLayer::Meta        => 0.3,
            CognitionLayer::Supervisory => 0.2,
            _                           => 1.0,
        }
    } else {
        match layer {
            CognitionLayer::Tactical    => TACTICAL_BUDGET,
            CognitionLayer::Strategic   => STRATEGIC_BUDGET,
            CognitionLayer::Meta        => META_BUDGET,
            CognitionLayer::Supervisory => SUPERVISORY_BUDGET,
            _                           => 1.0,
        }
    }
}

pub fn is_emergency() -> bool { EMERGENCY_SUPPRESSED.load(Ordering::Relaxed) }
pub fn is_bg_suspended() -> bool { BACKGROUND_SUSPENDED.load(Ordering::Relaxed) }

pub fn history(n: usize) -> Vec<SchedulerTickResult> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
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

    #[test]
    fn tick_returns_result() {
        let r = tick();
        assert_eq!(r.layer_budgets.len(), 5);
        assert!(SCHEDULER_TICKS.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn reactive_always_full_budget() {
        let _ = tick();
        assert_eq!(budget_for(CognitionLayer::Reactive), 1.0);
    }

    #[test]
    fn budget_for_tactical_positive() {
        let b = budget_for(CognitionLayer::Tactical);
        assert!(b > 0.0 && b <= 1.0);
    }
}
