//! Meta cognition layer — integrates Phase 18 live meta-cognition runtime.
//! Handles reasoning degradation events, uncertainty spikes, strategy changes.
//! Delegates heavy lifting to live_meta_loop and meta_cognition_runtime.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::{CognitionEvent, CognitionLayer, LayerResult};

pub static META_EVENTS:          AtomicU64 = AtomicU64::new(0);
pub static DEGRADATIONS_HANDLED: AtomicU64 = AtomicU64::new(0);
pub static SPIKES_HANDLED:       AtomicU64 = AtomicU64::new(0);
pub static STRATEGY_CHANGES:     AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 60;

// ── Meta record ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetaRecord {
    pub event:  String,
    pub detail: String,
    pub ts_ms:  u64,
}

struct MetaState {
    history: Vec<MetaRecord>,
}

static STATE: Lazy<Mutex<MetaState>> = Lazy::new(|| Mutex::new(MetaState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn process(event: &CognitionEvent) -> LayerResult {
    META_EVENTS.fetch_add(1, Ordering::Relaxed);
    let start = ts_now();

    match event {
        CognitionEvent::ReasoningDegraded { quality, cycle } => {
            DEGRADATIONS_HANDLED.fetch_add(1, Ordering::Relaxed);

            // Trigger a meta-cognition cycle to diagnose and recover
            let meta_result = crate::meta_cognition_runtime::run_cycle();

            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "reasoning_degraded",
                    &format!("cycle={cycle} q={quality:.3} meta_q={:.3}", meta_result.reasoning_quality));
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Meta,
                "reasoning:quality",
                format!("{quality:.3}"),
                *quality,
            );

            // Publish to meta event bus
            crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::ReasoningFailure {
                quality: *quality,
                cycle:   *cycle,
            });

            if *quality < 0.2 {
                LayerResult::escalate(CognitionLayer::Meta, event.label(),
                    CognitionLayer::Supervisory, ts_now() - start)
            } else {
                LayerResult::ok(CognitionLayer::Meta, event.label(), ts_now() - start)
            }
        }

        CognitionEvent::UncertaintySpike { dimension, value } => {
            SPIKES_HANDLED.fetch_add(1, Ordering::Relaxed);

            // Re-calibrate uncertainty immediately
            let calib = crate::live_uncertainty::calibrate();

            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "uncertainty_spike",
                    &format!("dim={dimension} v={value:.3} overall={:.3}", calib.overall));
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Meta,
                format!("uncertainty:{dimension}"),
                format!("{value:.3}"),
                1.0 - value,
            );

            if calib.critical_count > 0 {
                LayerResult::escalate(CognitionLayer::Meta, event.label(),
                    CognitionLayer::Supervisory, ts_now() - start)
            } else {
                LayerResult::ok(CognitionLayer::Meta, event.label(), ts_now() - start)
            }
        }

        CognitionEvent::StrategyChanged { old_strategy, new_strategy } => {
            STRATEGY_CHANGES.fetch_add(1, Ordering::Relaxed);

            // Run live counterfactual to validate the change
            let cf = crate::live_counterfactuals::evaluate();
            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "strategy_changed",
                    &format!("{old_strategy}→{new_strategy} cf_gain={:.3}", cf.best_improvement));
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Meta, "strategy:current", new_strategy.as_str(), 0.8);

            LayerResult::ok(CognitionLayer::Meta, event.label(), ts_now() - start)
        }

        _ => {
            LayerResult::escalate(CognitionLayer::Meta, event.label(),
                CognitionLayer::Supervisory, ts_now() - start)
        }
    }
}

/// Run a proactive meta evaluation (called by hierarchical_runtime on cadence).
pub fn evaluate() -> crate::meta_cognition_runtime::MetaCycleResult {
    let result = crate::meta_cognition_runtime::run_cycle();
    if let Ok(mut s) = STATE.lock() {
        record(&mut s.history, "proactive_eval",
            &format!("healthy={} q={:.3}", result.is_healthy(), result.reasoning_quality));
    }
    result
}

pub fn history(n: usize) -> Vec<MetaRecord> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

fn record(history: &mut Vec<MetaRecord>, event: &str, detail: &str) {
    if history.len() >= MAX_HISTORY { history.remove(0); }
    history.push(MetaRecord { event: event.to_string(), detail: detail.to_string(), ts_ms: ts_now() });
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
    fn reasoning_degraded_handled() {
        let ev = CognitionEvent::ReasoningDegraded { quality: 0.5, cycle: 1 };
        let result = process(&ev);
        assert!(result.handled || result.escalate.is_some());
        assert!(DEGRADATIONS_HANDLED.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn low_quality_reasoning_escalates() {
        let ev = CognitionEvent::ReasoningDegraded { quality: 0.1, cycle: 2 };
        let result = process(&ev);
        assert_eq!(result.escalate, Some(CognitionLayer::Supervisory));
    }

    #[test]
    fn strategy_changed_logged() {
        let ev = CognitionEvent::StrategyChanged {
            old_strategy: "conservative".into(),
            new_strategy: "balanced".into(),
        };
        let result = process(&ev);
        assert!(result.handled);
        assert!(STRATEGY_CHANGES.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn proactive_evaluate_returns_result() {
        let r = evaluate();
        assert!(r.reasoning_quality >= 0.0);
    }
}
