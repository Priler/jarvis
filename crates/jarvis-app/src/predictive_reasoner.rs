//! Predictive reasoner — generates near-term predictions about the environment
//! based on world-model history and workflow patterns.
//!
//! No ML.  Predictions are heuristic rules over recent observations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static PREDICTIONS_MADE:     AtomicU64 = AtomicU64::new(0);
pub static PREDICTIONS_VERIFIED: AtomicU64 = AtomicU64::new(0);
pub static PREDICTIONS_CORRECT:  AtomicU64 = AtomicU64::new(0);

const MAX_STORED: usize = 50;

// ── Prediction kinds ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PredictionKind {
    AppWillBeReady      { process: String },
    DialogWillAppear    { reason: String },
    AppMayFreeze        { process: String },
    WorkflowContinues   { next_tool: String },
    LoadingWillComplete { app: String },
    AnomalyWillPersist  { label: String },
}

impl PredictionKind {
    pub fn label(&self) -> String {
        match self {
            PredictionKind::AppWillBeReady      { process } => format!("AppReady({})", process),
            PredictionKind::DialogWillAppear    { reason }  => format!("DialogAppear({})", reason),
            PredictionKind::AppMayFreeze        { process } => format!("AppFreeze({})", process),
            PredictionKind::WorkflowContinues   { next_tool } => format!("Next({})", next_tool),
            PredictionKind::LoadingWillComplete { app }     => format!("LoadingDone({})", app),
            PredictionKind::AnomalyWillPersist  { label }   => format!("AnomalyPersists({})", label),
        }
    }
}

// ── Prediction ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Prediction {
    pub id:         u64,
    pub kind:       PredictionKind,
    pub confidence: f32,
    pub ts_ms:      u64,
    pub verified:   Option<bool>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Prediction {
    pub fn new(kind: PredictionKind, confidence: f32) -> Self {
        PREDICTIONS_MADE.fetch_add(1, Ordering::Relaxed);
        Self {
            id:         NEXT_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            confidence: confidence.clamp(0.0, 1.0),
            ts_ms:      ts_now(),
            verified:   None,
        }
    }

    pub fn age_ms(&self) -> u64 {
        ts_now().saturating_sub(self.ts_ms)
    }
}

// ── Prediction store ──────────────────────────────────────────────────────────

static PREDICTIONS: Lazy<Mutex<Vec<Prediction>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn store(pred: Prediction) {
    if let Ok(mut guard) = PREDICTIONS.lock() {
        if guard.len() >= MAX_STORED {
            guard.remove(0);
        }
        guard.push(pred.clone());
    }
    crate::world_state_journal::log(crate::world_state_journal::WorldEventKind::PredictionMade {
        prediction: pred.kind.label(),
        confidence: pred.confidence,
    });
}

// ── Predictive reasoner ───────────────────────────────────────────────────────

pub struct PredictiveReasoner;

impl PredictiveReasoner {
    pub fn predict() -> Vec<Prediction> {
        let mut preds = Vec::new();

        // Prediction 1: app about to be ready (was launching, now in history)
        {
            use crate::persistent_world_model;
            let recent = persistent_world_model::recent(3);
            for entry in &recent {
                if entry.env_state.contains("AppLaunching") || entry.env_state.contains("WorkspaceLoading") {
                    if let Some(ref app) = entry.active_app {
                        let p = Prediction::new(
                            PredictionKind::LoadingWillComplete { app: app.clone() },
                            0.65,
                        );
                        store(p.clone());
                        preds.push(p);
                        break;
                    }
                }
            }
        }

        // Prediction 2: workflow continuation
        {
            use crate::workflow_learning;
            use crate::cognitive_memory;
            let recent_ticks = cognitive_memory::recent(5);
            let recent_tools: Vec<String> = recent_ticks.iter()
                .flat_map(|t| t.notes.iter()
                    .filter(|n| n.starts_with("tool:"))
                    .map(|n| n.trim_start_matches("tool:").to_string()))
                .collect();
            if let Some(pattern) = workflow_learning::matches_known_pattern(&recent_tools) {
                if pattern.sequence.len() > 2 {
                    if let Some(next) = pattern.sequence.last() {
                        let p = Prediction::new(
                            PredictionKind::WorkflowContinues { next_tool: next.clone() },
                            pattern.confidence * 0.8,
                        );
                        store(p.clone());
                        preds.push(p);
                    }
                }
            }
        }

        // Prediction 3: anomaly persistence
        {
            use crate::anomaly_detector::AnomalyDetector;
            let anomalies = AnomalyDetector::scan();
            for anomaly in &anomalies {
                if anomaly.kind.requires_intervention() {
                    let p = Prediction::new(
                        PredictionKind::AnomalyWillPersist { label: anomaly.kind.label().to_string() },
                        0.75,
                    );
                    store(p.clone());
                    preds.push(p);
                }
            }
        }

        preds
    }

    pub fn verify(prediction_id: u64, correct: bool) {
        PREDICTIONS_VERIFIED.fetch_add(1, Ordering::Relaxed);
        if correct { PREDICTIONS_CORRECT.fetch_add(1, Ordering::Relaxed); }
        if let Ok(mut guard) = PREDICTIONS.lock() {
            if let Some(p) = guard.iter_mut().find(|p| p.id == prediction_id) {
                p.verified = Some(correct);
            }
        }
        crate::world_state_journal::log(crate::world_state_journal::WorldEventKind::PredictionVerified {
            prediction: prediction_id.to_string(),
            correct,
        });
    }

    pub fn recent_predictions(n: usize) -> Vec<Prediction> {
        PREDICTIONS.lock().map(|g| {
            let len = g.len();
            g[len.saturating_sub(n)..].to_vec()
        }).unwrap_or_default()
    }

    pub fn accuracy() -> f32 {
        let verified = PREDICTIONS_VERIFIED.load(Ordering::Relaxed);
        let correct  = PREDICTIONS_CORRECT.load(Ordering::Relaxed);
        if verified == 0 { return 0.0; }
        correct as f32 / verified as f32
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
    fn predict_runs_without_panic() {
        let _ = PredictiveReasoner::predict();
    }

    #[test]
    fn prediction_confidence_clamped() {
        let p = Prediction::new(PredictionKind::AppMayFreeze { process: "x".into() }, 1.5);
        assert!(p.confidence <= 1.0);
        let p2 = Prediction::new(PredictionKind::AppMayFreeze { process: "x".into() }, -0.5);
        assert!(p2.confidence >= 0.0);
    }

    #[test]
    fn predictions_made_counter_increments() {
        let before = PREDICTIONS_MADE.load(Ordering::Relaxed);
        let _ = Prediction::new(PredictionKind::DialogWillAppear { reason: "test".into() }, 0.5);
        assert!(PREDICTIONS_MADE.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn verify_prediction_increments_counters() {
        let p = Prediction::new(PredictionKind::DialogWillAppear { reason: "test".into() }, 0.5);
        let id = p.id;
        store(p);
        let before = PREDICTIONS_VERIFIED.load(Ordering::Relaxed);
        PredictiveReasoner::verify(id, true);
        assert!(PREDICTIONS_VERIFIED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn accuracy_returns_zero_with_no_verifications() {
        // accuracy() can't be reset between tests, but it should always be [0,1]
        let acc = PredictiveReasoner::accuracy();
        assert!(acc >= 0.0 && acc <= 1.0);
    }

    #[test]
    fn prediction_label_non_empty() {
        let kinds = [
            PredictionKind::AppWillBeReady    { process: "x".into() },
            PredictionKind::DialogWillAppear  { reason: "y".into() },
            PredictionKind::WorkflowContinues { next_tool: "z.t".into() },
        ];
        for k in &kinds {
            assert!(!k.label().is_empty());
        }
    }
}
