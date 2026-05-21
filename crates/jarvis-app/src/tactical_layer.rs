//! Tactical cognition layer — handles workflow execution, tool orchestration,
//! short-term planning, verification, and recovery.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::{CognitionEvent, CognitionLayer, LayerResult};

pub static TACTICAL_EVENTS:    AtomicU64 = AtomicU64::new(0);
pub static WORKFLOWS_TRACKED:  AtomicU64 = AtomicU64::new(0);
pub static RECOVERIES_HANDLED: AtomicU64 = AtomicU64::new(0);
pub static TOOLS_LOGGED:       AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

// ── Tactical state ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TacticalRecord {
    pub event:  String,
    pub detail: String,
    pub ts_ms:  u64,
}

struct TacticalState {
    active_workflows: Vec<String>,
    history:          Vec<TacticalRecord>,
}

static STATE: Lazy<Mutex<TacticalState>> = Lazy::new(|| Mutex::new(TacticalState {
    active_workflows: Vec::new(),
    history:          Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn process(event: &CognitionEvent) -> LayerResult {
    TACTICAL_EVENTS.fetch_add(1, Ordering::Relaxed);
    let start = ts_now();

    match event {
        CognitionEvent::WorkflowStarted { workflow_id } => {
            WORKFLOWS_TRACKED.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                if !s.active_workflows.contains(workflow_id) {
                    s.active_workflows.push(workflow_id.clone());
                }
                record(&mut s.history, "workflow_started", workflow_id);
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Tactical,
                format!("workflow:{workflow_id}"),
                "active",
                0.9,
            );
            // Record causal link: workflow_start → resource_usage
            crate::causal_reasoner::observe("workflow_start", "resource_usage", 0.6);
            LayerResult::ok(CognitionLayer::Tactical, event.label(), ts_now() - start)
        }

        CognitionEvent::WorkflowCompleted { workflow_id, success } => {
            if let Ok(mut s) = STATE.lock() {
                s.active_workflows.retain(|w| w != workflow_id);
                let detail = format!("{workflow_id}:success={success}");
                record(&mut s.history, "workflow_completed", &detail);
            }
            crate::workflow_learning::record_tool_execution(workflow_id.as_str());
            crate::hierarchical_memory::write(
                CognitionLayer::Tactical,
                format!("workflow:{workflow_id}"),
                if *success { "completed" } else { "failed" },
                if *success { 0.9 } else { 0.2 },
            );
            LayerResult::ok(CognitionLayer::Tactical, event.label(), ts_now() - start)
        }

        CognitionEvent::ToolExecuted { tool_id, success, latency_ms } => {
            TOOLS_LOGGED.fetch_add(1, Ordering::Relaxed);
            crate::workflow_learning::record_tool_execution(tool_id.as_str());
            if !success {
                crate::causal_reasoner::observe("tool_failure", "recovery_needed", 0.7);
            }
            if let Ok(mut s) = STATE.lock() {
                let detail = format!("{tool_id}:ok={success} lat={latency_ms}ms");
                record(&mut s.history, "tool_executed", &detail);
            }
            LayerResult::ok(CognitionLayer::Tactical, event.label(), ts_now() - start)
        }

        CognitionEvent::RecoveryTriggered { reason } => {
            RECOVERIES_HANDLED.fetch_add(1, Ordering::Relaxed);
            crate::causal_reasoner::observe("recovery_triggered", "stability_improved", 0.5);
            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "recovery", reason);
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Tactical, "recovery:last_reason", reason.as_str(), 0.7);
            LayerResult::ok(CognitionLayer::Tactical, event.label(), ts_now() - start)
        }

        CognitionEvent::CriticalAnomaly { kind, severity } if *severity < 0.9 => {
            // Moderate anomalies can be handled at tactical level
            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "anomaly_tactical", kind);
            }
            LayerResult::ok(CognitionLayer::Tactical, event.label(), ts_now() - start)
        }

        _ => {
            // Escalate unrecognised events to Strategic
            LayerResult::escalate(CognitionLayer::Tactical, event.label(),
                CognitionLayer::Strategic, ts_now() - start)
        }
    }
}

pub fn active_workflows() -> Vec<String> {
    STATE.lock().map(|s| s.active_workflows.clone()).unwrap_or_default()
}

pub fn history(n: usize) -> Vec<TacticalRecord> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

fn record(history: &mut Vec<TacticalRecord>, event: &str, detail: &str) {
    if history.len() >= MAX_HISTORY { history.remove(0); }
    history.push(TacticalRecord { event: event.to_string(), detail: detail.to_string(), ts_ms: ts_now() });
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
    fn workflow_started_tracked() {
        let ev = CognitionEvent::WorkflowStarted { workflow_id: "test_wf".into() };
        let result = process(&ev);
        assert!(result.handled);
        assert!(active_workflows().contains(&"test_wf".to_string()));
    }

    #[test]
    fn workflow_completed_removes_from_active() {
        let start = CognitionEvent::WorkflowStarted { workflow_id: "wf_done".into() };
        process(&start);
        let done = CognitionEvent::WorkflowCompleted { workflow_id: "wf_done".into(), success: true };
        let result = process(&done);
        assert!(result.handled);
        assert!(!active_workflows().contains(&"wf_done".to_string()));
    }

    #[test]
    fn tool_executed_logged() {
        let ev = CognitionEvent::ToolExecuted { tool_id: "click".into(), success: true, latency_ms: 50 };
        let before = TOOLS_LOGGED.load(Ordering::Relaxed);
        process(&ev);
        assert!(TOOLS_LOGGED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn strategic_event_escalates() {
        let ev = CognitionEvent::LongHorizonGoalAdded { goal_id: 1, description: "x".into() };
        let result = process(&ev);
        assert!(!result.handled);
        assert_eq!(result.escalate, Some(CognitionLayer::Strategic));
    }
}
