//! Cognition coordinator — orchestrates inter-layer message passing and
//! escalation resolution.  Dispatches events through the hierarchy until handled.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::{CognitionEvent, CognitionLayer, LayerResult};

pub static EVENTS_COORDINATED: AtomicU64 = AtomicU64::new(0);
pub static ESCALATION_CHAINS:  AtomicU64 = AtomicU64::new(0);
pub static HANDLED_AT_FIRST:   AtomicU64 = AtomicU64::new(0);

const MAX_ESCALATION_DEPTH: usize = 5;   // prevents runaway escalation chains

// ── Coordination result ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoordinationResult {
    pub event:         String,
    pub final_layer:   CognitionLayer,
    pub escalations:   usize,
    pub handled:       bool,
    pub total_ms:      u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct CoordState {
    history: Vec<CoordinationResult>,
}

static STATE: Lazy<Mutex<CoordState>> = Lazy::new(|| Mutex::new(CoordState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Route and process an event through the hierarchy.
/// Starts at the natural (or bypass) layer, escalates if needed.
pub fn dispatch(event: &CognitionEvent) -> CoordinationResult {
    EVENTS_COORDINATED.fetch_add(1, Ordering::Relaxed);
    let start = ts_now();
    let event_label = event.label().to_string();

    let (mut current_layer, _bypass) = crate::cognitive_router::route(event);
    let mut escalations = 0usize;
    let mut final_result = LayerResult::skip(current_layer, event.label());

    for _ in 0..MAX_ESCALATION_DEPTH {
        let layer_result = process_at_layer(current_layer, event);

        if layer_result.handled {
            if escalations == 0 { HANDLED_AT_FIRST.fetch_add(1, Ordering::Relaxed); }
            final_result = layer_result;
            break;
        }

        match layer_result.escalate {
            Some(next) => {
                ESCALATION_CHAINS.fetch_add(1, Ordering::Relaxed);
                escalations += 1;
                // Emit escalation event to supervisory for observability
                crate::generalized_observability::log(
                    crate::generalized_observability::HierarchyObs::Escalation {
                        from: current_layer,
                        to:   next,
                        event: event_label.clone(),
                    }
                );
                current_layer = next;
                final_result = layer_result;
            }
            None => {
                // Layer chose not to handle and not to escalate (skip)
                final_result = layer_result;
                break;
            }
        }
    }

    let total_ms = ts_now().saturating_sub(start);
    let result = CoordinationResult {
        event:       event_label,
        final_layer: current_layer,
        escalations,
        handled:     final_result.handled,
        total_ms,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= 200 { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

/// Dispatch multiple events (e.g., from a tick).
pub fn dispatch_batch(events: &[CognitionEvent]) -> Vec<CoordinationResult> {
    events.iter().map(|e| dispatch(e)).collect()
}

pub fn history(n: usize) -> Vec<CoordinationResult> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

// ── Layer dispatch ────────────────────────────────────────────────────────────

fn process_at_layer(layer: CognitionLayer, event: &CognitionEvent) -> LayerResult {
    match layer {
        CognitionLayer::Reactive    => crate::reactive_layer::process(event),
        CognitionLayer::Tactical    => crate::tactical_layer::process(event),
        CognitionLayer::Strategic   => crate::strategic_layer::process(event),
        CognitionLayer::Meta        => crate::meta_layer::process(event),
        CognitionLayer::Supervisory => crate::supervisory_layer::process(event),
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

    #[test]
    fn safety_interrupt_handled_at_reactive() {
        let ev = CognitionEvent::SafetyInterrupt { reason: "coord_test".into() };
        let result = dispatch(&ev);
        assert!(result.handled);
        assert_eq!(result.final_layer, CognitionLayer::Reactive);
        assert_eq!(result.escalations, 0);
        crate::reactive_layer::release_safety_lock();
    }

    #[test]
    fn tactical_event_handled_at_tactical() {
        let ev = CognitionEvent::WorkflowStarted { workflow_id: "coord_wf".into() };
        let result = dispatch(&ev);
        assert!(result.handled);
    }

    #[test]
    fn strategic_event_goes_to_strategic() {
        let ev = CognitionEvent::LongHorizonGoalAdded {
            goal_id: 99, description: "coord_goal".into() };
        let result = dispatch(&ev);
        assert!(result.handled);
    }

    #[test]
    fn batch_dispatch_all_handled() {
        let events = vec![
            CognitionEvent::WakeSignal { source: "test".into() },
            CognitionEvent::WorkflowStarted { workflow_id: "batch_wf".into() },
        ];
        let results = dispatch_batch(&events);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.handled));
    }
}
