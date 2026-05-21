//! Strategic cognition layer — long-horizon goals, multi-workflow coordination,
//! resource planning, environment optimization, future planning.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::{CognitionEvent, CognitionLayer, LayerResult};

pub static STRATEGIC_EVENTS:    AtomicU64 = AtomicU64::new(0);
pub static GOALS_REGISTERED:    AtomicU64 = AtomicU64::new(0);
pub static DRIFT_DETECTED:      AtomicU64 = AtomicU64::new(0);
pub static RESOURCE_PRESSURES:  AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 80;
const DRIFT_ESCALATION_THRESH: f32 = 0.75;

// ── Strategic record ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategicRecord {
    pub event:  String,
    pub detail: String,
    pub ts_ms:  u64,
}

struct StrategicState {
    history:          Vec<StrategicRecord>,
    resource_pressure: f32,
}

static STATE: Lazy<Mutex<StrategicState>> = Lazy::new(|| Mutex::new(StrategicState {
    history:          Vec::new(),
    resource_pressure: 0.0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn process(event: &CognitionEvent) -> LayerResult {
    STRATEGIC_EVENTS.fetch_add(1, Ordering::Relaxed);
    let start = ts_now();

    match event {
        CognitionEvent::LongHorizonGoalAdded { goal_id, description } => {
            GOALS_REGISTERED.fetch_add(1, Ordering::Relaxed);
            // Record in long_horizon_goals
            let id = crate::long_horizon_goals::add(
                crate::long_horizon_goals::HorizonKind::BackgroundObjective {
                    description: description.clone(),
                }
            );
            if let Ok(mut s) = STATE.lock() {
                let detail = format!("id={goal_id} lhg_id={id} desc={description}");
                record(&mut s.history, "lh_goal_added", &detail);
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Strategic,
                format!("goal:{goal_id}"),
                description.as_str(),
                0.85,
            );
            LayerResult::ok(CognitionLayer::Strategic, event.label(), ts_now() - start)
        }

        CognitionEvent::EnvironmentDrifted { drift_score } => {
            DRIFT_DETECTED.fetch_add(1, Ordering::Relaxed);
            crate::causal_reasoner::observe("env_drift", "strategy_adjustment", *drift_score);
            if let Ok(mut s) = STATE.lock() {
                record(&mut s.history, "env_drifted", &format!("score={drift_score:.3}"));
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Strategic, "env:drift_score",
                format!("{drift_score:.3}"), *drift_score);

            if *drift_score >= DRIFT_ESCALATION_THRESH {
                // High drift needs supervisory coordination
                LayerResult::escalate(CognitionLayer::Strategic, event.label(),
                    CognitionLayer::Supervisory, ts_now() - start)
            } else {
                LayerResult::ok(CognitionLayer::Strategic, event.label(), ts_now() - start)
            }
        }

        CognitionEvent::ResourcePressure { resource, pressure } => {
            RESOURCE_PRESSURES.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut s) = STATE.lock() {
                s.resource_pressure = s.resource_pressure.max(*pressure);
                record(&mut s.history, "resource_pressure",
                    &format!("{resource}={pressure:.3}"));
            }
            crate::hierarchical_memory::write(
                CognitionLayer::Strategic,
                format!("resource:{resource}"),
                format!("{pressure:.3}"),
                *pressure,
            );
            if *pressure >= 0.9 {
                LayerResult::escalate(CognitionLayer::Strategic, event.label(),
                    CognitionLayer::Supervisory, ts_now() - start)
            } else {
                LayerResult::ok(CognitionLayer::Strategic, event.label(), ts_now() - start)
            }
        }

        _ => {
            LayerResult::escalate(CognitionLayer::Strategic, event.label(),
                CognitionLayer::Supervisory, ts_now() - start)
        }
    }
}

pub fn current_resource_pressure() -> f32 {
    STATE.lock().map(|s| s.resource_pressure).unwrap_or(0.0)
}

pub fn history(n: usize) -> Vec<StrategicRecord> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

fn record(history: &mut Vec<StrategicRecord>, event: &str, detail: &str) {
    if history.len() >= MAX_HISTORY { history.remove(0); }
    history.push(StrategicRecord { event: event.to_string(), detail: detail.to_string(), ts_ms: ts_now() });
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
    fn lh_goal_added_handled() {
        let ev = CognitionEvent::LongHorizonGoalAdded { goal_id: 1, description: "test goal".into() };
        let result = process(&ev);
        assert!(result.handled);
        assert_eq!(result.escalate, None);
        assert!(GOALS_REGISTERED.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn low_drift_handled_locally() {
        let ev = CognitionEvent::EnvironmentDrifted { drift_score: 0.3 };
        let result = process(&ev);
        assert!(result.handled);
        assert!(result.escalate.is_none());
    }

    #[test]
    fn high_drift_escalates_to_supervisory() {
        let ev = CognitionEvent::EnvironmentDrifted { drift_score: 0.9 };
        let result = process(&ev);
        assert_eq!(result.escalate, Some(CognitionLayer::Supervisory));
    }

    #[test]
    fn resource_pressure_tracked() {
        let ev = CognitionEvent::ResourcePressure { resource: "cpu".into(), pressure: 0.6 };
        process(&ev);
        assert!(current_resource_pressure() >= 0.6);
    }
}
