//! Bayesian strategy engine — compares probabilistic plans, estimates expected
//! stability and risk, and selects best plan by posterior confidence.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_SCORES: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── BayesianPlanScore ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BayesianPlanScore {
    pub plan_id:              String,
    pub expected_stability:   f32,
    pub expected_risk:        f32,
    pub posterior_confidence: f32,
    pub recommended:          bool,
    pub ts_ms:                u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

static SCORES: Lazy<Mutex<Vec<BayesianPlanScore>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Scoring ───────────────────────────────────────────────────────────────────

pub fn score_plans() -> Vec<BayesianPlanScore> {
    let plans    = crate::generalized_planner::active_plans();
    let conf_rep = crate::confidence_reasoner::assess();
    let unc      = crate::generalized_uncertainty::profile();

    let scores: Vec<BayesianPlanScore> = plans.iter().map(|p| {
        let plan_risk         = p.risk * 0.50 + unc.overall * 0.50;
        let expected_stability = (1.0 - plan_risk).clamp(0.0, 1.0);
        let posterior_confidence = (conf_rep.overall * 0.60 + expected_stability * 0.40).clamp(0.0, 1.0);
        BayesianPlanScore {
            plan_id:             p.id.clone(),
            expected_stability,
            expected_risk:       plan_risk.clamp(0.0, 1.0),
            posterior_confidence,
            recommended:         posterior_confidence > 0.60 && expected_stability > 0.50,
            ts_ms:               ts_now(),
        }
    }).collect();

    let mut ranked = scores.clone();
    ranked.sort_by(|a, b| b.posterior_confidence.partial_cmp(&a.posterior_confidence).unwrap());

    let mut s = SCORES.lock().unwrap();
    for sc in &ranked {
        if s.len() >= MAX_SCORES { s.remove(0); }
        s.push(sc.clone());
    }
    ranked
}

pub fn best_plan() -> Option<BayesianPlanScore> {
    score_plans().into_iter().find(|s| s.recommended)
}

pub fn estimate_recovery_probability() -> f32 {
    let conf = crate::confidence_reasoner::latest()
        .map(|r| r.overall)
        .unwrap_or(0.50);
    let unc = crate::generalized_uncertainty::latest()
        .map(|p| p.overall)
        .unwrap_or(0.40);
    (conf * 0.60 + (1.0 - unc) * 0.40).clamp(0.0, 1.0)
}

pub fn recent_scores(n: usize) -> Vec<BayesianPlanScore> {
    SCORES.lock().unwrap().iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_no_panic() {
        let _ = score_plans();
    }

    #[test]
    fn recovery_prob_in_range() {
        let p = estimate_recovery_probability();
        assert!(p >= 0.0 && p <= 1.0);
    }
}
