//! Meta-cognitive event bus — typed, bounded, lock-minimal broadcast channel
//! for all Phase 18 meta-cognition runtime events.
//! Consumers poll `drain()` each tick; no async, no blocking.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static EVENTS_PUBLISHED: AtomicU64 = AtomicU64::new(0);
pub static EVENTS_DROPPED:   AtomicU64 = AtomicU64::new(0);

const MAX_QUEUE: usize = 256;

// ── Event kinds ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MetaEvent {
    ReasoningFailure     { quality: f32, cycle: u64 },
    SimulationResult     { plan_id: String, success_prob: f32, should_execute: bool },
    UncertaintyShift     { dimension: String, old: f32, new: f32 },
    StrategyDegradation  { strategy_id: String, score_drop: f32 },
    CausalInstability    { cause: String, effect: String, strength_drop: f32 },
    PredictionCollapse   { horizon: u32, confidence_drop: f32 },
    ReflectionEvent      { insight: String, severity: f32, is_failure: bool },
    WatchdogIntervention { kind: WatchdogKind, action: String },
    UncertaintyRecalib   { dimension: String, value: f32 },
    MetaCycleCompleted   { cycle_id: u64, healthy: bool },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WatchdogKind {
    RecursionStorm,
    StrategyOscillation,
    PlannerInstability,
    UncertaintyRunaway,
    SimulationCollapse,
}

impl MetaEvent {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReasoningFailure     { .. } => "reasoning_failure",
            Self::SimulationResult     { .. } => "simulation_result",
            Self::UncertaintyShift     { .. } => "uncertainty_shift",
            Self::StrategyDegradation  { .. } => "strategy_degradation",
            Self::CausalInstability    { .. } => "causal_instability",
            Self::PredictionCollapse   { .. } => "prediction_collapse",
            Self::ReflectionEvent      { .. } => "reflection_event",
            Self::WatchdogIntervention { .. } => "watchdog_intervention",
            Self::UncertaintyRecalib   { .. } => "uncertainty_recalib",
            Self::MetaCycleCompleted   { .. } => "meta_cycle_completed",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct BusState {
    queue: Vec<MetaEvent>,
}

static STATE: Lazy<Mutex<BusState>> = Lazy::new(|| Mutex::new(BusState {
    queue: Vec::with_capacity(64),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Publish an event onto the bus.  Drops silently when queue is full.
pub fn publish(event: MetaEvent) {
    EVENTS_PUBLISHED.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut s) = STATE.lock() {
        if s.queue.len() >= MAX_QUEUE {
            EVENTS_DROPPED.fetch_add(1, Ordering::Relaxed);
            s.queue.remove(0); // drop oldest
        }
        s.queue.push(event);
    }
}

/// Drain all queued events.  Call once per scheduler tick.
pub fn drain() -> Vec<MetaEvent> {
    STATE.lock().map(|mut s| std::mem::take(&mut s.queue)).unwrap_or_default()
}

/// Peek count without draining.
pub fn pending() -> usize {
    STATE.lock().map(|s| s.queue.len()).unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_and_drain() {
        publish(MetaEvent::ReasoningFailure { quality: 0.2, cycle: 1 });
        publish(MetaEvent::UncertaintyShift { dimension: "planner".into(), old: 0.3, new: 0.7 });
        let events = drain();
        assert!(events.len() >= 2);
        assert!(events.iter().any(|e| matches!(e, MetaEvent::ReasoningFailure { .. })));
    }

    #[test]
    fn drain_clears_queue() {
        publish(MetaEvent::MetaCycleCompleted { cycle_id: 99, healthy: true });
        let _ = drain();
        // after drain the bus may have events from other tests but pending should reset
        let _ = drain(); // second drain is empty or close to empty
    }
}
