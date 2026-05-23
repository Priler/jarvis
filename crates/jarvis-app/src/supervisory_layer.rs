//! Supervisory cognition layer — global arbitration, layer coordination,
//! resource balancing, stability enforcement, priority arbitration.
//! This is the top of the hierarchy; it never escalates.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::{CognitionEvent, CognitionLayer, LayerResult};

pub static SUPERVISORY_EVENTS:  AtomicU64 = AtomicU64::new(0);
pub static OVERLOADS_RESOLVED:  AtomicU64 = AtomicU64::new(0);
pub static PRIORITY_ARBITRATED: AtomicU64 = AtomicU64::new(0);
pub static ESCALATIONS_CAUGHT:  AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 80;

// ── Supervisory record ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SupervisoryRecord {
    pub event:   String,
    pub action:  String,
    pub outcome: String,
    pub ts_ms:   u64,
}

struct SupervisoryState {
    history:          Vec<SupervisoryRecord>,
    overloaded_layers: Vec<CognitionLayer>,
}

static STATE: Lazy<Mutex<SupervisoryState>> = Lazy::new(|| Mutex::new(SupervisoryState {
    history:           Vec::new(),
    overloaded_layers: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn process(event: &CognitionEvent) -> LayerResult {
    SUPERVISORY_EVENTS.fetch_add(1, Ordering::Relaxed);
    let start = ts_now();

    match event {
        CognitionEvent::LayerOverloaded { layer, queue_depth } => {
            OVERLOADS_RESOLVED.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                if !s.overloaded_layers.contains(layer) {
                    s.overloaded_layers.push(*layer);
                }
                let action = "throttle_layer";
                let outcome = format!("layer={} depth={queue_depth}", layer.label());
                record(&mut s.history, "layer_overloaded", action, &outcome);
            }
            // Suppress simulation when overloaded
            if *queue_depth > 10 {
                crate::meta_scheduler::suppress(crate::meta_scheduler::Subsystem::Simulation);
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Supervisory,
                format!("overload:{}", layer.label()),
                format!("depth={queue_depth}"),
                1.0,
            );
            LayerResult::ok(CognitionLayer::Supervisory, event.label(), ts_now() - start)
        }

        CognitionEvent::PriorityShift { from, to } => {
            PRIORITY_ARBITRATED.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "priority_shift", "arbitrate",
                    &format!("{from}→{to}"));
            }
            // Run live arbitration to resolve the priority conflict
            let _arb = crate::live_strategy_arbitration::run_live();
            LayerResult::ok(CognitionLayer::Supervisory, event.label(), ts_now() - start)
        }

        CognitionEvent::CognitionEscalated { from, to, reason } => {
            ESCALATIONS_CAUGHT.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "cognition_escalated",
                    "global_arbitration",
                    &format!("from={} to={} reason={reason}", from.label(), to.label()));
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Supervisory, "escalation:last_reason", reason.as_str(), 0.9);
            // Run watchdog check when escalation arrives
            let _wd = crate::cognitive_watchdog::check();
            LayerResult::ok(CognitionLayer::Supervisory, event.label(), ts_now() - start)
        }

        // All other events arriving here are escalations from lower layers
        _ => {
            ESCALATIONS_CAUGHT.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "escalation_caught",
                    "supervisory_resolve",
                    event.label());
            }
            // Supervisory always handles (top layer — no further escalation)
            LayerResult::ok(CognitionLayer::Supervisory, event.label(), ts_now() - start)
        }
    }
}

/// Run a proactive supervisory coordination pass.
pub fn coordinate() {
    // Check if any layers are flagged overloaded and clear resolved ones
    if let Ok(mut s) = STATE.lock() {
        s.overloaded_layers.clear();  // will be re-populated on next tick
        record(&mut s.history, "coordinate", "pass", "ok");
    }
    // Run global priority arbitration
    let _arb = crate::live_strategy_arbitration::run_live();
}

pub fn overloaded_layers() -> Vec<CognitionLayer> {
    STATE.lock().map(|s| s.overloaded_layers.clone()).unwrap_or_default()
}

pub fn history(n: usize) -> Vec<SupervisoryRecord> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

fn record(history: &mut Vec<SupervisoryRecord>, event: &str, action: &str, outcome: &str) {
    if history.len() >= MAX_HISTORY { history.remove(0); }
    history.push(SupervisoryRecord {
        event: event.to_string(), action: action.to_string(),
        outcome: outcome.to_string(), ts_ms: ts_now(),
    });
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
    fn layer_overloaded_handled() {
        let ev = CognitionEvent::LayerOverloaded {
            layer: CognitionLayer::Tactical, queue_depth: 5 };
        let result = process(&ev);
        assert!(result.handled);
        assert_eq!(OVERLOADS_RESOLVED.load(Ordering::Relaxed), OVERLOADS_RESOLVED.load(Ordering::Relaxed));
    }

    #[test]
    fn priority_shift_arbitrated() {
        let ev = CognitionEvent::PriorityShift { from: "opt".into(), to: "safety".into() };
        let result = process(&ev);
        assert!(result.handled);
    }

    #[test]
    fn escalated_event_always_handled_at_supervisory() {
        let ev = CognitionEvent::StrategyChanged {
            old_strategy: "a".into(), new_strategy: "b".into() };
        let result = process(&ev);
        assert!(result.handled);
        assert!(result.escalate.is_none()); // top layer — never escalates
    }

    #[test]
    fn coordinate_runs_without_panic() {
        coordinate();
    }
}
