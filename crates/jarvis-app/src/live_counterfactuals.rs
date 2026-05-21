//! Live counterfactual reasoning — continuously evaluates "what alternative
//! strategy would have worked better?" from accumulated runtime history.
//! Publishes ReflectionEvent notices when a better alternative is found.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static CF_EVALUATIONS:     AtomicU64 = AtomicU64::new(0);
pub static BETTER_FOUND:       AtomicU64 = AtomicU64::new(0);
pub static CF_RECOMMENDATIONS: AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 50;

// ── Live counterfactual result ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveCfResult {
    pub eval_id:           u64,
    pub baseline_quality:  f32,
    pub best_alternative:  Option<String>,
    pub best_improvement:  f32,
    pub recommendation:    String,
    pub scenarios_tested:  usize,
    pub ts_ms:             u64,
}

impl LiveCfResult {
    pub fn has_improvement(&self) -> bool { self.best_improvement > 0.05 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct CfState {
    history: Vec<LiveCfResult>,
    eval_id: u64,
}

static STATE: Lazy<Mutex<CfState>> = Lazy::new(|| Mutex::new(CfState {
    history: Vec::new(),
    eval_id: 0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Evaluate live counterfactuals from current runtime state.
pub fn evaluate() -> LiveCfResult {
    CF_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let eval_id = STATE.lock().map(|mut s| { s.eval_id += 1; s.eval_id }).unwrap_or(0);

    // Derive baseline quality from current meta-cognition state
    let meta_hist = crate::meta_cognition_runtime::recent_results();
    let baseline_quality = if meta_hist.is_empty() {
        0.5
    } else {
        meta_hist.iter().map(|r| r.reasoning_quality).sum::<f32>() / meta_hist.len() as f32
    };

    // Build candidate alternative strategies using counterfactual_runtime
    let stability = crate::cognitive_stability::check();
    let unc       = crate::uncertainty_engine::sample();

    let alternatives: &[(&str, f32)] = &[
        // (label, projected_quality_if_applied)
        ("reduce_simulation_frequency",  baseline_quality + if unc.overall > 0.6 { -0.05 } else { 0.08 }),
        ("increase_causal_weight",       baseline_quality + if stability.is_stable { 0.06 } else { 0.02 }),
        ("defer_optimization_goals",     baseline_quality + if unc.high_count > 2  { 0.10 } else { 0.01 }),
        ("tighten_watchdog_thresholds",  baseline_quality + if stability.is_unstable() { 0.07 } else { -0.02 }),
        ("hold_current_strategy",        baseline_quality),
    ];

    let mut best_alt: Option<&str>  = None;
    let mut best_gain: f32          = 0.0;

    for (label, projected) in alternatives {
        let gain = projected - baseline_quality;
        if gain > best_gain {
            best_gain = gain;
            best_alt  = Some(label);
        }
    }

    if best_gain > 0.05 {
        BETTER_FOUND.fetch_add(1, Ordering::Relaxed);
    }

    let recommendation = match best_alt {
        Some(alt) if best_gain > 0.05 => {
            CF_RECOMMENDATIONS.fetch_add(1, Ordering::Relaxed);
            crate::meta_event_bus::publish(crate::meta_event_bus::MetaEvent::ReflectionEvent {
                insight:    format!("cf_better_strategy:{alt}"),
                severity:   (best_gain * 2.0).clamp(0.0, 1.0),
                is_failure: false,
            });
            format!("apply:{alt} (proj. gain +{:.3})", best_gain)
        }
        _ => "hold_current_strategy".to_string(),
    };

    let result = LiveCfResult {
        eval_id,
        baseline_quality,
        best_alternative: best_alt.map(String::from),
        best_improvement: best_gain,
        recommendation,
        scenarios_tested: alternatives.len(),
        ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

pub fn history() -> Vec<LiveCfResult> {
    STATE.lock().map(|s| s.history.clone()).unwrap_or_default()
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
    fn evaluate_returns_result() {
        let r = evaluate();
        assert!(r.scenarios_tested > 0);
        assert!(CF_EVALUATIONS.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn recommendation_is_non_empty() {
        let r = evaluate();
        assert!(!r.recommendation.is_empty());
    }
}
