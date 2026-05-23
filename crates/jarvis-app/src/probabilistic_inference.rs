//! Probabilistic inference engine — ranks hypotheses, estimates failure
//! probability, semantic reliability, and future instability.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HYPOTHESES: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Hypothesis ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProbabilisticHypothesis {
    pub label:           String,
    pub probability:     f32,
    pub support_count:   usize,
    pub evidence_weight: f32,
    pub ts_ms:           u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct InfStore { hypotheses: Vec<ProbabilisticHypothesis> }

static STORE: Lazy<Mutex<InfStore>> = Lazy::new(|| Mutex::new(InfStore { hypotheses: Vec::new() }));

// ── Ranking ───────────────────────────────────────────────────────────────────

pub fn rank_hypotheses() -> Vec<ProbabilisticHypothesis> {
    let chains  = crate::symbolic_inference::reliable_chains();
    let beliefs = crate::belief_engine::reliable_beliefs();

    let mut hypotheses: Vec<ProbabilisticHypothesis> = Vec::new();

    // From symbolic inference chains (discount 20% for probabilistic uncertainty)
    for chain in &chains {
        let prob = (chain.confidence * 0.80).clamp(0.0, 1.0);
        if let Some(h) = hypotheses.iter_mut().find(|h| h.label == chain.conclusion) {
            h.support_count += 1;
            h.probability    = (h.probability * 0.70 + prob * 0.30).clamp(0.0, 1.0);
            h.evidence_weight = (h.evidence_weight + chain.confidence) * 0.50;
        } else {
            hypotheses.push(ProbabilisticHypothesis {
                label:           chain.conclusion.clone(),
                probability:     prob,
                support_count:   1,
                evidence_weight: chain.confidence,
                ts_ms:           ts_now(),
            });
        }
    }

    // From reliable beliefs (discount 30%)
    for b in &beliefs {
        let prob = (b.effective_confidence() * 0.70).clamp(0.0, 1.0);
        if let Some(h) = hypotheses.iter_mut().find(|h| h.label == b.label) {
            h.probability    = (h.probability * 0.60 + prob * 0.40).clamp(0.0, 1.0);
            h.support_count += 1;
        } else {
            hypotheses.push(ProbabilisticHypothesis {
                label:           b.label.clone(),
                probability:     prob,
                support_count:   1,
                evidence_weight: b.causal_support,
                ts_ms:           ts_now(),
            });
        }
    }

    hypotheses.sort_by(|a, b| b.probability.partial_cmp(&a.probability).unwrap());
    hypotheses.truncate(MAX_HYPOTHESES);

    let mut s = STORE.lock().unwrap();
    s.hypotheses = hypotheses.clone();
    hypotheses
}

pub fn estimate_failure_probability(component: &str) -> f32 {
    let unc = crate::generalized_uncertainty::latest();
    let stability = crate::semantic_stability::check();
    let base = match component {
        "planner"  => unc.as_ref().map(|u| u.planner_uncertainty).unwrap_or(0.30),
        "semantic" => unc.as_ref().map(|u| u.semantic_uncertainty).unwrap_or(0.30),
        "workflow" => unc.as_ref().map(|u| u.workflow_uncertainty).unwrap_or(0.30),
        "causal"   => unc.as_ref().map(|u| u.causal_uncertainty).unwrap_or(0.30),
        _          => stability.instability_score,
    };
    base.clamp(0.0, 1.0)
}

pub fn estimate_semantic_reliability() -> f32 {
    let stability = crate::semantic_stability::check();
    (1.0 - stability.instability_score).clamp(0.0, 1.0)
}

pub fn estimate_future_instability() -> f32 {
    let profile   = crate::generalized_uncertainty::latest().map(|p| p.overall).unwrap_or(0.40);
    let stability = crate::semantic_stability::check();
    (profile * 0.60 + stability.instability_score * 0.40).clamp(0.0, 1.0)
}

pub fn top_hypotheses(n: usize) -> Vec<ProbabilisticHypothesis> {
    STORE.lock().unwrap().hypotheses.iter().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_no_panic() {
        let _ = rank_hypotheses();
    }

    #[test]
    fn estimates_in_range() {
        assert!(estimate_failure_probability("planner") <= 1.0);
        assert!(estimate_semantic_reliability()         >= 0.0);
        assert!(estimate_future_instability()           <= 1.0);
    }
}
