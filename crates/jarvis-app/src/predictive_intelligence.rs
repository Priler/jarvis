//! Predictive intelligence — forecasts failures, cognition instability,
//! workflow degradation, and strategic collapse risks probabilistically.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_PREDICTIONS: usize = 200;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── PredictedEvent ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredictionSeverity { Low, Medium, High, Critical }

#[derive(Debug, Clone)]
pub struct PredictedEvent {
    pub label:                 String,
    pub probability:           f32,
    pub estimated_ticks_until: Option<u32>,
    pub severity:              PredictionSeverity,
    pub ts_ms:                 u64,
}

impl PredictedEvent {
    pub fn is_actionable(&self) -> bool {
        matches!(self.severity, PredictionSeverity::High | PredictionSeverity::Critical)
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static STORE: Lazy<Mutex<Vec<PredictedEvent>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn classify_severity(p: f32) -> PredictionSeverity {
    if p > 0.80      { PredictionSeverity::Critical }
    else if p > 0.60 { PredictionSeverity::High }
    else if p > 0.40 { PredictionSeverity::Medium }
    else             { PredictionSeverity::Low }
}

// ── Predictions ───────────────────────────────────────────────────────────────

pub fn predict_failures() -> Vec<PredictedEvent> {
    let unc        = crate::generalized_uncertainty::profile();
    let self_model = crate::semantic_self_model::sample();
    let stability  = crate::semantic_stability::check();

    let mut events: Vec<PredictedEvent> = Vec::new();

    // Planner failure risk
    let planner_p = (unc.planner_uncertainty * 0.80 + self_model.cognitive_degradation * 0.20).clamp(0.0, 1.0);
    if planner_p > 0.25 {
        let ticks = (1.0 / planner_p.max(0.01)).round() as u32;
        events.push(PredictedEvent {
            label:                 "planner_failure".into(),
            probability:           planner_p,
            estimated_ticks_until: Some(ticks),
            severity:              classify_severity(planner_p),
            ts_ms:                 ts_now(),
        });
    }

    // Semantic instability
    let semantic_p = (stability.instability_score * 0.70 + unc.semantic_uncertainty * 0.30).clamp(0.0, 1.0);
    if semantic_p > 0.25 {
        events.push(PredictedEvent {
            label:                 "semantic_instability".into(),
            probability:           semantic_p,
            estimated_ticks_until: None,
            severity:              classify_severity(semantic_p),
            ts_ms:                 ts_now(),
        });
    }

    // Workflow degradation
    let workflow_p = unc.workflow_uncertainty;
    if workflow_p > 0.25 {
        events.push(PredictedEvent {
            label:                 "workflow_degradation".into(),
            probability:           workflow_p,
            estimated_ticks_until: None,
            severity:              classify_severity(workflow_p),
            ts_ms:                 ts_now(),
        });
    }

    // Semantic collapse (immediate if risk flag set)
    if stability.has_collapse_risk {
        events.push(PredictedEvent {
            label:                 "semantic_collapse_risk".into(),
            probability:           0.90,
            estimated_ticks_until: Some(1),
            severity:              PredictionSeverity::Critical,
            ts_ms:                 ts_now(),
        });
    }

    let mut store = STORE.lock().unwrap();
    for e in &events {
        if store.len() >= MAX_PREDICTIONS { store.remove(0); }
        store.push(e.clone());
    }

    crate::probabilistic_observability::log(
        crate::probabilistic_observability::ProbabilisticEvent::PredictionMade {
            label:       "batch_failure_prediction".into(),
            probability: events.iter().map(|e| e.probability).fold(0.0_f32, f32::max),
        }
    );

    events
}

pub fn predict_cognition_instability() -> f32 {
    crate::semantic_self_model::sample().cognitive_degradation
}

pub fn predict_workflow_degradation() -> f32 {
    crate::generalized_uncertainty::profile().workflow_uncertainty
}

pub fn predict_strategic_collapse_risk() -> f32 {
    let unc       = crate::generalized_uncertainty::profile();
    let stability = crate::semantic_stability::check();
    (unc.overall * 0.60 + stability.instability_score * 0.40).clamp(0.0, 1.0)
}

pub fn recent_predictions(n: usize) -> Vec<PredictedEvent> {
    STORE.lock().unwrap().iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predict_failures_no_panic() {
        let _ = predict_failures();
    }

    #[test]
    fn instability_in_range() {
        let v = predict_cognition_instability();
        assert!(v >= 0.0 && v <= 1.0);
    }
}
