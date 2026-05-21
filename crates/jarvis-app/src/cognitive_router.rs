//! Cognitive router — determines which cognition layer handles each event,
//! including escalation and de-escalation paths.
//! Priority: critical events always bypass to Reactive layer first.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::{CognitionEvent, CognitionLayer, LayerResult};

pub static EVENTS_DISPATCHED:   AtomicU64 = AtomicU64::new(0);
pub static BYPASSES_TO_REACTIVE: AtomicU64 = AtomicU64::new(0);
pub static ESCALATIONS_ROUTED:  AtomicU64 = AtomicU64::new(0);

const MAX_ROUTE_HISTORY: usize = 200;

// ── Route record ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouteRecord {
    pub event:  String,
    pub target: CognitionLayer,
    pub bypass: bool,
    pub ts_ms:  u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct RouterState {
    history: Vec<RouteRecord>,
}

static STATE: Lazy<Mutex<RouterState>> = Lazy::new(|| Mutex::new(RouterState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Determine the target layer for an event.  Returns target + whether it was a bypass.
pub fn route(event: &CognitionEvent) -> (CognitionLayer, bool) {
    EVENTS_DISPATCHED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    // Safety bypass: critical events always go to Reactive regardless of natural layer
    let (target, bypass) = if event.is_critical() {
        BYPASSES_TO_REACTIVE.fetch_add(1, Ordering::Relaxed);
        (CognitionLayer::Reactive, true)
    } else {
        (event.natural_layer(), false)
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_ROUTE_HISTORY { s.history.remove(0); }
        s.history.push(RouteRecord { event: event.label().to_string(), target, bypass, ts_ms: now });
    }

    (target, bypass)
}

/// Escalation path: given a layer that couldn't handle an event, return the next layer up.
pub fn escalation_target(from: CognitionLayer) -> Option<CognitionLayer> {
    ESCALATIONS_ROUTED.fetch_add(1, Ordering::Relaxed);
    match from {
        CognitionLayer::Reactive    => Some(CognitionLayer::Tactical),
        CognitionLayer::Tactical    => Some(CognitionLayer::Strategic),
        CognitionLayer::Strategic   => Some(CognitionLayer::Supervisory),
        CognitionLayer::Meta        => Some(CognitionLayer::Supervisory),
        CognitionLayer::Supervisory => None,   // top of hierarchy
    }
}

/// De-escalation path: given a resolved result, return the layer to notify.
pub fn de_escalation_target(from: CognitionLayer) -> Option<CognitionLayer> {
    match from {
        CognitionLayer::Supervisory => Some(CognitionLayer::Strategic),
        CognitionLayer::Strategic   => Some(CognitionLayer::Tactical),
        CognitionLayer::Tactical    => Some(CognitionLayer::Reactive),
        _                           => None,
    }
}

pub fn recent_routes(limit: usize) -> Vec<RouteRecord> {
    STATE.lock().map(|s| s.history.iter().rev().take(limit).cloned().collect()).unwrap_or_default()
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
    fn safety_interrupt_bypasses_to_reactive() {
        let ev = CognitionEvent::SafetyInterrupt { reason: "halt".into() };
        let (layer, bypass) = route(&ev);
        assert_eq!(layer, CognitionLayer::Reactive);
        assert!(bypass);
    }

    #[test]
    fn normal_event_routes_to_natural_layer() {
        let ev = CognitionEvent::WorkflowStarted { workflow_id: "wf1".into() };
        let (layer, bypass) = route(&ev);
        assert_eq!(layer, CognitionLayer::Tactical);
        assert!(!bypass);
    }

    #[test]
    fn escalation_path_is_correct() {
        assert_eq!(escalation_target(CognitionLayer::Reactive), Some(CognitionLayer::Tactical));
        assert_eq!(escalation_target(CognitionLayer::Supervisory), None);
    }

    #[test]
    fn route_recorded_in_history() {
        let ev = CognitionEvent::WakeSignal { source: "test".into() };
        route(&ev);
        let hist = recent_routes(5);
        assert!(hist.iter().any(|r| r.event == "wake_signal"));
    }
}
