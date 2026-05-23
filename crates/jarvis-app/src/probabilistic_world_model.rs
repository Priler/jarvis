//! Probabilistic world model — represents environment state probabilistically:
//! workflow stability, causal reliability, confidence-weighted dependencies.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── State variants ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ProbabilisticEnvironmentState {
    HighlyConfident,   // overall > 0.80
    Confident,         // overall > 0.60
    Uncertain,         // overall > 0.40
    Unreliable,        // overall > 0.20
    Collapsed,         // overall <= 0.20
}

impl ProbabilisticEnvironmentState {
    pub fn label(&self) -> &str {
        match self {
            Self::HighlyConfident => "highly_confident",
            Self::Confident       => "confident",
            Self::Uncertain       => "uncertain",
            Self::Unreliable      => "unreliable",
            Self::Collapsed       => "collapsed",
        }
    }
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProbabilisticWorldSnapshot {
    pub environment_confidence:  f32,
    pub workflow_stability_prob: f32,
    pub causal_reliability:      f32,
    pub belief_coherence:        f32,
    pub uncertainty_level:       f32,
    pub state:                   ProbabilisticEnvironmentState,
    pub unstable_components:     Vec<String>,
    pub ts_ms:                   u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<ProbabilisticWorldSnapshot>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Snapshot ──────────────────────────────────────────────────────────────────

pub fn snapshot() -> ProbabilisticWorldSnapshot {
    let resource  = crate::abstract_resource_reasoner::sample();
    let unc       = crate::uncertainty_engine::sample();
    let stability = crate::semantic_stability::check();
    let causal    = crate::causal_reasoner::reliable_links();

    let causal_reliability = if causal.is_empty() {
        0.50
    } else {
        let sum: f32 = causal.iter().map(|l| l.strength).sum();
        (sum / causal.len() as f32).clamp(0.0, 1.0)
    };

    let workflow_stability_prob = (1.0 - unc.overall).clamp(0.0, 1.0);
    let belief_coherence        = crate::belief_engine::avg_confidence();
    let uncertainty_level       = crate::uncertainty_graph::avg_uncertainty();

    let environment_confidence  = (belief_coherence      * 0.30
        + workflow_stability_prob                         * 0.25
        + causal_reliability                              * 0.25
        + (1.0 - resource.overall)                       * 0.20)
        .clamp(0.0, 1.0);

    let state = if environment_confidence > 0.80      { ProbabilisticEnvironmentState::HighlyConfident }
        else if environment_confidence > 0.60         { ProbabilisticEnvironmentState::Confident }
        else if environment_confidence > 0.40         { ProbabilisticEnvironmentState::Uncertain }
        else if environment_confidence > 0.20         { ProbabilisticEnvironmentState::Unreliable }
        else                                          { ProbabilisticEnvironmentState::Collapsed };

    let mut unstable = crate::uncertainty_graph::unstable_nodes();
    if stability.has_collapse_risk  { unstable.push("semantic_graph".into()); }
    if stability.has_recursion_risk { unstable.push("inference_depth".into()); }
    if resource.is_overloaded()     { unstable.push("resource_layer".into()); }

    let snap = ProbabilisticWorldSnapshot {
        environment_confidence,
        workflow_stability_prob,
        causal_reliability,
        belief_coherence,
        uncertainty_level,
        state,
        unstable_components: unstable,
        ts_ms: ts_now(),
    };

    let mut h = HISTORY.lock().unwrap();
    if h.len() >= MAX_HISTORY { h.remove(0); }
    h.push(snap.clone());
    snap
}

pub fn latest() -> Option<ProbabilisticWorldSnapshot> {
    HISTORY.lock().unwrap().last().cloned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_valid_confidence() {
        let s = snapshot();
        assert!(s.environment_confidence >= 0.0 && s.environment_confidence <= 1.0);
    }

    #[test]
    fn state_label_not_empty() {
        let s = snapshot();
        assert!(!s.state.label().is_empty());
    }
}
