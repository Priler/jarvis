//! Attention runtime — determines which part of the environment the cognition
//! loop should focus on during the current tick.
//!
//! Priority is based on: anomaly severity, active goals, recent changes, and
//! environment state.  Attention never triggers actions — it guides the planner.

use std::sync::atomic::{AtomicU64, Ordering};

pub static ATTENTION_EVALUATIONS: AtomicU64 = AtomicU64::new(0);
pub static ATTENTION_SHIFTS:      AtomicU64 = AtomicU64::new(0);

// ── Attention priority ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum AttentionPriority {
    Critical = 0,
    High     = 1,
    Normal   = 2,
    Low      = 3,
    Idle     = 4,
}

impl std::fmt::Display for AttentionPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// ── Attention focus ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AttentionFocus {
    Anomaly       { label: String },
    ActiveGoal    { goal_id: u64, description: String },
    EnvironmentChange { description: String },
    WorkflowStep  { tool_id: String },
    Idle,
}

impl AttentionFocus {
    pub fn description(&self) -> String {
        match self {
            AttentionFocus::Anomaly        { label }       => format!("anomaly: {}", label),
            AttentionFocus::ActiveGoal     { description, .. } => format!("goal: {}", description),
            AttentionFocus::EnvironmentChange { description } => format!("env change: {}", description),
            AttentionFocus::WorkflowStep   { tool_id }     => format!("workflow: {}", tool_id),
            AttentionFocus::Idle                           => "idle".to_string(),
        }
    }
}

// ── Attention decision ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttentionDecision {
    pub priority:  AttentionPriority,
    pub focus:     AttentionFocus,
    pub rationale: String,
    pub ts_ms:     u64,
}

impl AttentionDecision {
    pub fn idle() -> Self {
        Self {
            priority:  AttentionPriority::Idle,
            focus:     AttentionFocus::Idle,
            rationale: "no active goals or anomalies".to_string(),
            ts_ms:     ts_now(),
        }
    }

    pub fn is_idle(&self) -> bool {
        self.priority == AttentionPriority::Idle
    }
}

// ── Attention runtime ─────────────────────────────────────────────────────────

pub struct AttentionRuntime;

impl AttentionRuntime {
    pub fn evaluate() -> AttentionDecision {
        ATTENTION_EVALUATIONS.fetch_add(1, Ordering::Relaxed);

        // Priority 1: critical anomalies
        {
            use crate::anomaly_detector::AnomalyDetector;
            let anomalies = AnomalyDetector::scan();
            if let Some(critical) = anomalies.iter().find(|a| a.kind.requires_intervention()) {
                ATTENTION_SHIFTS.fetch_add(1, Ordering::Relaxed);
                crate::world_state_journal::log(
                    crate::world_state_journal::WorldEventKind::AttentionShifted {
                        from: None,
                        to:   critical.kind.label().to_string(),
                        priority: "Critical".to_string(),
                    },
                );
                return AttentionDecision {
                    priority:  AttentionPriority::Critical,
                    focus:     AttentionFocus::Anomaly { label: critical.kind.label().to_string() },
                    rationale: format!("critical anomaly: {}", critical.evidence),
                    ts_ms:     ts_now(),
                };
            }
        }

        // Priority 2: active goals
        {
            use crate::goal_runtime::GoalRuntime;
            let goals = GoalRuntime::active_goals();
            if let Some(goal) = goals.first() {
                return AttentionDecision {
                    priority:  AttentionPriority::High,
                    focus:     AttentionFocus::ActiveGoal {
                        goal_id:     goal.id,
                        description: goal.kind.description(),
                    },
                    rationale: format!("active goal #{}: {}", goal.id, goal.kind.description()),
                    ts_ms:     ts_now(),
                };
            }
        }

        // Priority 3: recent environment changes
        {
            use crate::active_observer::ActiveObserver;
            let obs = ActiveObserver::observe();
            if obs.has_changes() {
                let desc = if obs.changes.has_dialog_change() {
                    "dialog state changed"
                } else if obs.changes.has_focus_change() {
                    "window focus changed"
                } else {
                    "environment changed"
                };
                return AttentionDecision {
                    priority:  AttentionPriority::Normal,
                    focus:     AttentionFocus::EnvironmentChange { description: desc.to_string() },
                    rationale: desc.to_string(),
                    ts_ms:     ts_now(),
                };
            }
        }

        // Priority 4: workflow prediction
        {
            use crate::predictive_reasoner::PredictiveReasoner;
            let preds = PredictiveReasoner::recent_predictions(1);
            if let Some(pred) = preds.first() {
                if pred.confidence >= 0.6 {
                    use crate::predictive_reasoner::PredictionKind;
                    if let PredictionKind::WorkflowContinues { ref next_tool } = pred.kind {
                        return AttentionDecision {
                            priority:  AttentionPriority::Low,
                            focus:     AttentionFocus::WorkflowStep { tool_id: next_tool.clone() },
                            rationale: format!("predicted next workflow step: {}", next_tool),
                            ts_ms:     ts_now(),
                        };
                    }
                }
            }
        }

        AttentionDecision::idle()
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
    fn evaluate_returns_decision() {
        let decision = AttentionRuntime::evaluate();
        assert!(decision.ts_ms > 0);
    }

    #[test]
    fn idle_decision_is_idle() {
        let idle = AttentionDecision::idle();
        assert!(idle.is_idle());
        assert_eq!(idle.priority, AttentionPriority::Idle);
    }

    #[test]
    fn attention_priority_ordering() {
        assert!(AttentionPriority::Critical < AttentionPriority::High);
        assert!(AttentionPriority::High     < AttentionPriority::Normal);
        assert!(AttentionPriority::Idle     > AttentionPriority::Low);
    }

    #[test]
    fn attention_evaluations_counter_increments() {
        let before = ATTENTION_EVALUATIONS.load(Ordering::Relaxed);
        AttentionRuntime::evaluate();
        assert!(ATTENTION_EVALUATIONS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn focus_description_non_empty() {
        let foci = [
            AttentionFocus::Anomaly        { label: "FrozenApp".into() },
            AttentionFocus::ActiveGoal     { goal_id: 1, description: "open IDE".into() },
            AttentionFocus::EnvironmentChange { description: "window changed".into() },
            AttentionFocus::Idle,
        ];
        for f in &foci {
            assert!(!f.description().is_empty());
        }
    }

    #[test]
    fn evaluate_without_goals_or_anomalies_does_not_panic() {
        use crate::goal_runtime::GoalRuntime;
        GoalRuntime::clear_all();
        let _decision = AttentionRuntime::evaluate();
    }
}
