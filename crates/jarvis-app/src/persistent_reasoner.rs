//! Persistent reasoner — maintains accumulated reasoning state across ticks.
//!
//! Unlike per-tick reasoning (environment_reasoner), this module tracks
//! multi-tick inferences: trends, sustained states, and cross-tick conclusions.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static REASONER_UPDATES:    AtomicU64 = AtomicU64::new(0);
pub static REASONER_INFERENCES: AtomicU64 = AtomicU64::new(0);

// ── Persistent inference ──────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum InferenceKind {
    AppStable        { process: String, stable_since_ms: u64 },
    AppUnstable      { process: String, instability_count: u32 },
    WorkflowActive   { description: String },
    NoUserActivity   { idle_since_ms: u64 },
    SustainedLoading { app: String, since_ms: u64 },
    EnvironmentStable,
    EnvironmentDegraded { reason: String },
}

impl InferenceKind {
    pub fn label(&self) -> &'static str {
        match self {
            InferenceKind::AppStable { .. }        => "AppStable",
            InferenceKind::AppUnstable { .. }      => "AppUnstable",
            InferenceKind::WorkflowActive { .. }   => "WorkflowActive",
            InferenceKind::NoUserActivity { .. }   => "NoUserActivity",
            InferenceKind::SustainedLoading { .. } => "SustainedLoading",
            InferenceKind::EnvironmentStable       => "EnvironmentStable",
            InferenceKind::EnvironmentDegraded { .. } => "EnvironmentDegraded",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistentInference {
    pub kind:       InferenceKind,
    pub confidence: f32,
    pub first_seen_ms: u64,
    pub last_seen_ms:  u64,
    pub tick_count:    u32,
}

impl PersistentInference {
    pub fn new(kind: InferenceKind, confidence: f32) -> Self {
        let now = ts_now();
        Self { kind, confidence, first_seen_ms: now, last_seen_ms: now, tick_count: 1 }
    }

    pub fn reinforce(&mut self, confidence: f32) {
        self.last_seen_ms = ts_now();
        self.tick_count   += 1;
        // Weighted average toward new confidence
        self.confidence = (self.confidence * 0.7 + confidence * 0.3).min(1.0);
    }

    pub fn duration_ms(&self) -> u64 {
        self.last_seen_ms.saturating_sub(self.first_seen_ms)
    }

    pub fn is_sustained(&self) -> bool {
        self.tick_count >= 3
    }
}

// ── Reasoner state ────────────────────────────────────────────────────────────

struct ReasonerState {
    inferences: Vec<PersistentInference>,
    tick_count: u64,
}

static STATE: Lazy<Mutex<ReasonerState>> = Lazy::new(|| Mutex::new(ReasonerState {
    inferences: Vec::new(),
    tick_count: 0,
}));

// ── Persistent reasoner ───────────────────────────────────────────────────────

pub struct PersistentReasoner;

impl PersistentReasoner {
    pub fn update() -> Vec<PersistentInference> {
        REASONER_UPDATES.fetch_add(1, Ordering::Relaxed);

        let mut new_inferences = Vec::new();

        // Observation 1: environment stability
        {
            use crate::persistent_world_model;
            let recent = persistent_world_model::recent(5);
            if recent.len() >= 3 {
                let all_ready = recent.iter().all(|e| e.env_state.contains("Ready"));
                if all_ready {
                    let inf = PersistentInference::new(InferenceKind::EnvironmentStable, 0.85);
                    new_inferences.push(inf);
                }
                let any_degraded = recent.iter().any(|e| {
                    e.env_state.contains("Crash") || e.env_state.contains("Permission")
                });
                if any_degraded {
                    let reason = recent.iter()
                        .find(|e| e.env_state.contains("Crash") || e.env_state.contains("Permission"))
                        .map(|e| e.env_state.clone())
                        .unwrap_or_else(|| "degraded".to_string());
                    new_inferences.push(PersistentInference::new(
                        InferenceKind::EnvironmentDegraded { reason },
                        0.80,
                    ));
                }
            }
        }

        // Observation 2: sustained loading
        {
            use crate::persistent_world_model;
            let recent = persistent_world_model::recent(4);
            let all_loading = !recent.is_empty() && recent.iter().all(|e| {
                e.env_state.contains("Loading") || e.env_state.contains("Launching")
            });
            if all_loading {
                let app = recent.last().and_then(|e| e.active_app.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let since = recent.first().map(|e| e.ts_ms).unwrap_or_else(ts_now);
                new_inferences.push(PersistentInference::new(
                    InferenceKind::SustainedLoading { app, since_ms: since },
                    0.70,
                ));
            }
        }

        // Merge new inferences into state
        if let Ok(mut state) = STATE.lock() {
            state.tick_count += 1;
            for new_inf in &new_inferences {
                let label = new_inf.kind.label();
                if let Some(existing) = state.inferences.iter_mut()
                    .find(|i| i.kind.label() == label)
                {
                    existing.reinforce(new_inf.confidence);
                } else {
                    state.inferences.push(new_inf.clone());
                    REASONER_INFERENCES.fetch_add(1, Ordering::Relaxed);
                }
            }
            // Prune inferences not seen in recent 20 ticks
            state.inferences.retain(|i| {
                ts_now().saturating_sub(i.last_seen_ms) < 60_000
            });
        }

        new_inferences
    }

    pub fn current_inferences() -> Vec<PersistentInference> {
        STATE.lock().map(|s| s.inferences.clone()).unwrap_or_default()
    }

    pub fn has_inference(label: &str) -> bool {
        STATE.lock().map(|s| s.inferences.iter().any(|i| i.kind.label() == label))
            .unwrap_or(false)
    }

    pub fn tick_count() -> u64 {
        STATE.lock().map(|s| s.tick_count).unwrap_or(0)
    }

    pub fn clear() {
        if let Ok(mut state) = STATE.lock() {
            state.inferences.clear();
            state.tick_count = 0;
        }
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
    fn update_runs_without_panic() {
        let _ = PersistentReasoner::update();
    }

    #[test]
    fn tick_count_increments() {
        let before = PersistentReasoner::tick_count();
        PersistentReasoner::update();
        assert!(PersistentReasoner::tick_count() > before);
    }

    #[test]
    fn inference_label_consistent() {
        let inf = InferenceKind::AppStable { process: "x".into(), stable_since_ms: 0 };
        assert_eq!(inf.label(), "AppStable");
    }

    #[test]
    fn inference_reinforce_increases_tick_count() {
        let mut inf = PersistentInference::new(InferenceKind::EnvironmentStable, 0.7);
        assert_eq!(inf.tick_count, 1);
        inf.reinforce(0.9);
        assert_eq!(inf.tick_count, 2);
    }

    #[test]
    fn reasoner_updates_counter_increments() {
        let before = REASONER_UPDATES.load(Ordering::Relaxed);
        PersistentReasoner::update();
        assert!(REASONER_UPDATES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn sustained_inference_flag() {
        let mut inf = PersistentInference::new(InferenceKind::EnvironmentStable, 0.8);
        assert!(!inf.is_sustained());
        inf.tick_count = 3;
        assert!(inf.is_sustained());
    }
}
