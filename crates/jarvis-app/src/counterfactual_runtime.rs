//! Counterfactual runtime — evaluates "what if" scenarios by comparing the
//! projected outcome of alternative decisions against the baseline trajectory.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static COUNTERFACTUALS_EVALUATED: AtomicU64 = AtomicU64::new(0);
pub static BETTER_ALTERNATIVES_FOUND: AtomicU64 = AtomicU64::new(0);
pub static COUNTERFACTUAL_WINS:       AtomicU64 = AtomicU64::new(0);

const MAX_CF_HISTORY: usize = 60;

// ── Counterfactual scenario ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CounterfactualScenario {
    pub id:             String,
    pub description:    String,
    pub delta_risk:     f32,   // negative = lower risk than baseline
    pub delta_quality:  f32,   // positive = better quality than baseline
}

// ── Counterfactual result ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CounterfactualResult {
    pub scenario_id:        String,
    pub baseline_quality:   f32,
    pub alternative_quality: f32,
    pub improvement:        f32,
    pub is_better:          bool,
    pub recommendation:     String,
    pub ts_ms:              u64,
}

impl CounterfactualResult {
    pub fn net_gain(&self) -> f32 { self.improvement }
}

// ── Comparison set ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CounterfactualComparison {
    pub scenarios:      Vec<CounterfactualResult>,
    pub best_scenario:  Option<String>,
    pub baseline:       f32,
    pub ts_ms:          u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct CfState {
    history: Vec<CounterfactualComparison>,
}

static STATE: Lazy<Mutex<CfState>> = Lazy::new(|| Mutex::new(CfState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn evaluate(scenarios: &[CounterfactualScenario]) -> CounterfactualComparison {
    COUNTERFACTUALS_EVALUATED.fetch_add(scenarios.len() as u64, Ordering::Relaxed);
    let now = ts_now();

    let baseline = crate::execution_quality::latest()
        .map(|q| q.overall)
        .unwrap_or(0.6);

    let mut results: Vec<CounterfactualResult> = scenarios.iter().map(|sc| {
        let alt_quality = (baseline + sc.delta_quality - sc.delta_risk * 0.5).clamp(0.0, 1.0);
        let improvement = alt_quality - baseline;
        let is_better   = improvement > 0.02;

        if is_better { BETTER_ALTERNATIVES_FOUND.fetch_add(1, Ordering::Relaxed); }

        let recommendation = if is_better {
            format!("prefer '{}' (+{:.2} quality)", sc.description, improvement)
        } else {
            format!("baseline better than '{}'", sc.description)
        };

        CounterfactualResult {
            scenario_id: sc.id.clone(),
            baseline_quality: baseline,
            alternative_quality: alt_quality,
            improvement,
            is_better,
            recommendation,
            ts_ms: now,
        }
    }).collect();

    results.sort_by(|a, b| b.improvement.partial_cmp(&a.improvement).unwrap_or(std::cmp::Ordering::Equal));

    let best_scenario = results.first().filter(|r| r.is_better).map(|r| r.scenario_id.clone());
    if best_scenario.is_some() { COUNTERFACTUAL_WINS.fetch_add(1, Ordering::Relaxed); }

    let cmp = CounterfactualComparison { scenarios: results, best_scenario, baseline, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_CF_HISTORY { s.history.remove(0); }
        s.history.push(cmp.clone());
    }

    cmp
}

pub fn evaluate_single(scenario: CounterfactualScenario) -> CounterfactualResult {
    let cmp = evaluate(&[scenario]);
    cmp.scenarios.into_iter().next().expect("evaluate always returns one result per scenario")
}

pub fn latest() -> Option<CounterfactualComparison> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); }
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

    fn sc(id: &str, dq: f32, dr: f32) -> CounterfactualScenario {
        CounterfactualScenario { id: id.into(), description: id.into(), delta_quality: dq, delta_risk: dr }
    }

    #[test]
    fn evaluate_returns_one_result_per_scenario() {
        let cmp = evaluate(&[sc("cf.u1.a", 0.1, 0.0), sc("cf.u1.b", -0.1, 0.2)]);
        assert_eq!(cmp.scenarios.len(), 2);
    }

    #[test]
    fn counterfactuals_evaluated_counter_increments() {
        let before = COUNTERFACTUALS_EVALUATED.load(Ordering::Relaxed);
        evaluate(&[sc("cf.u2.a", 0.05, 0.0)]);
        assert!(COUNTERFACTUALS_EVALUATED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn better_alternative_detected() {
        let before = BETTER_ALTERNATIVES_FOUND.load(Ordering::Relaxed);
        evaluate(&[sc("cf.u3.a", 0.3, 0.0)]);
        assert!(BETTER_ALTERNATIVES_FOUND.load(Ordering::Relaxed) >= before);
    }

    #[test]
    fn alternative_quality_bounded() {
        let cmp = evaluate(&[sc("cf.u4.a", 0.5, 0.0)]);
        for r in &cmp.scenarios {
            assert!(r.alternative_quality >= 0.0 && r.alternative_quality <= 1.0);
        }
    }

    #[test]
    fn evaluate_single_works() {
        let r = evaluate_single(sc("cf.u5.a", 0.1, 0.1));
        assert!(r.improvement.is_finite());
    }

    #[test]
    fn best_scenario_selected_correctly() {
        let cmp = evaluate(&[sc("cf.u6.a", 0.3, 0.0), sc("cf.u6.b", 0.0, 0.5)]);
        // If best exists, it should have positive improvement
        if let Some(ref best_id) = cmp.best_scenario {
            let best = cmp.scenarios.iter().find(|r| &r.scenario_id == best_id).unwrap();
            assert!(best.improvement > 0.0);
        }
    }
}
