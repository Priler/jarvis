//! Probabilistic observability — structured event log for belief evolution,
//! confidence propagation, uncertainty drift, and self-model changes.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_LOG: usize = 500;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Event variants ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ProbabilisticEvent {
    BeliefAsserted       { label: String, confidence: f32 },
    BeliefDecayed        { label: String, from: f32, to: f32 },
    ConfidencePropagated { from: String, to: String, delta: f32 },
    UncertaintyDrift     { delta: f32, direction: String },
    InferenceCompleted   { hypothesis: String, probability: f32 },
    SelfModelUpdated     { component: String, old_val: f32, new_val: f32 },
    PredictionMade       { label: String, probability: f32 },
    SafetyGateFired      { reason: String },
}

impl ProbabilisticEvent {
    pub fn severity(&self) -> f32 {
        match self {
            Self::SafetyGateFired { .. }                                          => 0.90,
            Self::UncertaintyDrift { delta, .. } if *delta > 0.30                => 0.75,
            Self::BeliefDecayed { from, to, .. } if (from - to) > 0.20           => 0.65,
            Self::InferenceCompleted { probability, .. } if *probability < 0.30  => 0.60,
            Self::SelfModelUpdated { .. }                                         => 0.40,
            Self::ConfidencePropagated { .. }                                     => 0.20,
            _                                                                     => 0.15,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::BeliefAsserted { .. }       => "BeliefAsserted",
            Self::BeliefDecayed { .. }        => "BeliefDecayed",
            Self::ConfidencePropagated { .. } => "ConfidencePropagated",
            Self::UncertaintyDrift { .. }     => "UncertaintyDrift",
            Self::InferenceCompleted { .. }   => "InferenceCompleted",
            Self::SelfModelUpdated { .. }     => "SelfModelUpdated",
            Self::PredictionMade { .. }       => "PredictionMade",
            Self::SafetyGateFired { .. }      => "SafetyGateFired",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ObsState {
    events: Vec<(u64, ProbabilisticEvent)>,
}

static STATE: Lazy<Mutex<ObsState>> = Lazy::new(|| Mutex::new(ObsState { events: Vec::new() }));

// ── API ───────────────────────────────────────────────────────────────────────

pub fn log(event: ProbabilisticEvent) {
    let mut s = STATE.lock().unwrap();
    if s.events.len() >= MAX_LOG { s.events.remove(0); }
    s.events.push((ts_now(), event));
}

pub fn recent(n: usize) -> Vec<(u64, ProbabilisticEvent)> {
    STATE.lock().unwrap().events.iter().rev().take(n).cloned().collect()
}

pub fn event_count() -> usize { STATE.lock().unwrap().events.len() }

pub fn high_severity_count(threshold: f32) -> usize {
    STATE.lock().unwrap().events.iter().filter(|(_, e)| e.severity() >= threshold).count()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_and_retrieve() {
        log(ProbabilisticEvent::BeliefAsserted { label: "test_obs_ph22".into(), confidence: 0.7 });
        assert!(event_count() >= 1);
    }

    #[test]
    fn safety_gate_has_high_severity() {
        let e = ProbabilisticEvent::SafetyGateFired { reason: "test".into() };
        assert_eq!(e.severity(), 0.90);
    }
}
