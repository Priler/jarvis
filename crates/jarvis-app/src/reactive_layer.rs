//! Reactive cognition layer — fastest layer; handles critical anomalies, safety
//! interrupts, and wake signals with deterministic, low-latency logic.
//! Must NEVER call into deep reasoning. Bypasses all scheduler cadences.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::{CognitionEvent, CognitionLayer, LayerResult};

pub static REACTIVE_EVENTS:     AtomicU64 = AtomicU64::new(0);
pub static SAFETY_INTERRUPTS:   AtomicU64 = AtomicU64::new(0);
pub static ANOMALY_REACTIONS:   AtomicU64 = AtomicU64::new(0);
pub static WAKE_RESPONSES:      AtomicU64 = AtomicU64::new(0);

const MAX_REACTION_HISTORY: usize = 50;

// ── Reaction record ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReactiveAction {
    pub trigger:    String,
    pub action:     String,
    pub severity:   f32,
    pub ts_ms:      u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ReactiveState {
    history: Vec<ReactiveAction>,
    safety_locked: bool,
}

static STATE: Lazy<Mutex<ReactiveState>> = Lazy::new(|| Mutex::new(ReactiveState {
    history:       Vec::new(),
    safety_locked: false,
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Process an event at the reactive layer.
/// Returns a LayerResult — may escalate to Tactical if unrecognised.
pub fn process(event: &CognitionEvent) -> LayerResult {
    REACTIVE_EVENTS.fetch_add(1, Ordering::Relaxed);
    let start = ts_now();

    match event {
        CognitionEvent::SafetyInterrupt { reason } => {
            SAFETY_INTERRUPTS.fetch_add(1, Ordering::Relaxed);
            // Deterministic: set safety lock, no deep reasoning
            if let Ok(mut s) = STATE.lock() {
                s.safety_locked = true;
                record_action(&mut s.history, reason.as_str(), "safety_lock_engaged", 1.0);
            }
            // Write to reactive memory
            crate::hierarchical_memory::write(
                CognitionLayer::Reactive, "safety:locked", "true", 1.0);
            LayerResult::ok(CognitionLayer::Reactive, event.label(), ts_now() - start)
        }

        CognitionEvent::CriticalAnomaly { kind, severity } => {
            ANOMALY_REACTIONS.fetch_add(1, Ordering::Relaxed);
            let action = if *severity >= 0.9 {
                "emergency_halt_requested"
            } else {
                "anomaly_logged_escalate"
            };
            if let Ok(mut s) = STATE.lock() {
                record_action(&mut s.history, kind.as_str(), action, *severity);
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Reactive,
                format!("anomaly:{kind}"),
                format!("severity={severity:.3}"),
                *severity,
            );
            if *severity >= 0.9 {
                LayerResult::ok(CognitionLayer::Reactive, event.label(), ts_now() - start)
            } else {
                // Escalate moderate anomalies to Tactical for full analysis
                LayerResult::escalate(CognitionLayer::Reactive, event.label(),
                    CognitionLayer::Tactical, ts_now() - start)
            }
        }

        CognitionEvent::WakeSignal { source } => {
            WAKE_RESPONSES.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                record_action(&mut s.history, source.as_str(), "wake_acknowledged", 0.5);
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Reactive, "wake:last_source", source.as_str(), 0.8);
            LayerResult::ok(CognitionLayer::Reactive, event.label(), ts_now() - start)
        }

        _ => {
            // Reactive layer does not handle this event; escalate
            LayerResult::escalate(CognitionLayer::Reactive, event.label(),
                CognitionLayer::Tactical, ts_now() - start)
        }
    }
}

pub fn is_safety_locked() -> bool {
    STATE.lock().map(|s| s.safety_locked).unwrap_or(false)
}

pub fn release_safety_lock() {
    if let Ok(mut s) = STATE.lock() { s.safety_locked = false; }
}

pub fn recent_actions(n: usize) -> Vec<ReactiveAction> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

fn record_action(history: &mut Vec<ReactiveAction>, trigger: &str, action: &str, severity: f32) {
    if history.len() >= MAX_REACTION_HISTORY { history.remove(0); }
    history.push(ReactiveAction { trigger: trigger.to_string(), action: action.to_string(), severity, ts_ms: ts_now() });
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
    fn safety_interrupt_locks_and_returns_ok() {
        release_safety_lock();
        let ev = CognitionEvent::SafetyInterrupt { reason: "test_halt".into() };
        let result = process(&ev);
        assert!(result.handled);
        assert!(is_safety_locked());
        release_safety_lock();
    }

    #[test]
    fn critical_anomaly_high_severity_handled() {
        let ev = CognitionEvent::CriticalAnomaly { kind: "oom".into(), severity: 0.95 };
        let result = process(&ev);
        assert!(result.handled);
        assert!(result.escalate.is_none());
    }

    #[test]
    fn moderate_anomaly_escalates() {
        let ev = CognitionEvent::CriticalAnomaly { kind: "lag".into(), severity: 0.7 };
        let result = process(&ev);
        assert!(!result.handled);
        assert_eq!(result.escalate, Some(CognitionLayer::Tactical));
    }

    #[test]
    fn tactical_event_escalates_from_reactive() {
        let ev = CognitionEvent::WorkflowStarted { workflow_id: "wf1".into() };
        let result = process(&ev);
        assert!(!result.handled);
    }
}
