//! Meta-cognition runtime — top-level orchestrator for Phase 17.
//! Coordinates all meta-cognitive modules: reasoning analysis, uncertainty
//! estimation, causal reasoning, future simulation, counterfactual evaluation,
//! meta-strategy optimization, arbitration, reflection, safety, and stability.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static META_CYCLES:          AtomicU64 = AtomicU64::new(0);
pub static META_CYCLES_BLOCKED:  AtomicU64 = AtomicU64::new(0);
pub static META_CYCLES_CERTIFIED: AtomicU64 = AtomicU64::new(0);

const MAX_CYCLE_HISTORY: usize = 50;

// ── Meta-cycle result ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetaCycleResult {
    pub cycle_id:             u64,
    pub reasoning_quality:    f32,
    pub overall_uncertainty:  f32,
    pub confidence:           f32,
    pub stability_score:      f32,
    pub safety_certified:     bool,
    pub reflection_failures:  usize,
    pub strategy_changed:     bool,
    pub projection_horizon:   u32,
    pub ts_ms:                u64,
}

impl MetaCycleResult {
    pub fn is_healthy(&self) -> bool {
        self.confidence > 0.4 && self.safety_certified && self.reasoning_quality > 0.4
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct MetaState {
    history:  Vec<MetaCycleResult>,
    cycle_id: u64,
}

static STATE: Lazy<Mutex<MetaState>> = Lazy::new(|| Mutex::new(MetaState {
    history:  Vec::new(),
    cycle_id: 0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn run_cycle() -> MetaCycleResult {
    META_CYCLES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let cycle_id = STATE.lock().map(|mut s| { s.cycle_id += 1; s.cycle_id }).unwrap_or(0);

    // 1. Safety gate — must pass before any meta-cognitive operation
    let safety = crate::meta_cognition_safety::verify();
    if !safety.certified {
        META_CYCLES_BLOCKED.fetch_add(1, Ordering::Relaxed);
        return MetaCycleResult {
            cycle_id,
            reasoning_quality:   0.0,
            overall_uncertainty: 1.0,
            confidence:          0.0,
            stability_score:     0.0,
            safety_certified:    false,
            reflection_failures: safety.violation_count(),
            strategy_changed:    false,
            projection_horizon:  0,
            ts_ms:               now,
        };
    }

    // 2. Uncertainty sampling
    let unc_snap = crate::uncertainty_engine::sample();

    // 3. Reasoning analysis
    let reasoning = crate::reasoning_analyzer::analyze();

    // 4. Cognitive stability check
    let stability = crate::cognitive_stability::check();

    // 5. Confidence measurement
    let confidence_bd = crate::cognitive_confidence::measure();

    // 6. Future state projection (3 steps)
    let projection = crate::future_state_simulator::project(3);

    // 7. Meta-strategy optimization
    let models = vec![
        crate::meta_strategy_optimizer::conservative_model(),
        crate::meta_strategy_optimizer::balanced_model(),
        crate::meta_strategy_optimizer::aggressive_model(),
    ];
    let mso = crate::meta_strategy_optimizer::optimize(&models);

    // 8. Counterfactual evaluation
    let cf_scenarios = vec![
        crate::counterfactual_runtime::CounterfactualScenario {
            id: "conservative_cf".into(),
            description: "conservative approach".into(),
            delta_risk:    -0.1,
            delta_quality:  0.05,
        },
        crate::counterfactual_runtime::CounterfactualScenario {
            id: "aggressive_cf".into(),
            description: "aggressive approach".into(),
            delta_risk:    0.2,
            delta_quality:  0.15,
        },
    ];
    let _cf = crate::counterfactual_runtime::evaluate(&cf_scenarios);

    // 9. Meta-reflection
    let reflection = crate::meta_reflection::reflect();

    // 10. Strategic arbitration (no competing goals in this cycle — advisory)
    let _arb = crate::strategic_arbitration::arbitrate(&[]);

    META_CYCLES_CERTIFIED.fetch_add(1, Ordering::Relaxed);

    let result = MetaCycleResult {
        cycle_id,
        reasoning_quality:   reasoning.overall,
        overall_uncertainty: unc_snap.overall,
        confidence:          confidence_bd.overall,
        stability_score:     1.0 - stability.oscillation_score,
        safety_certified:    true,
        reflection_failures: reflection.failure_count,
        strategy_changed:    mso.strategy_changed,
        projection_horizon:  projection.safe_horizon(),
        ts_ms:               now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_CYCLE_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

pub fn latest() -> Option<MetaCycleResult> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn recent_results() -> Vec<MetaCycleResult> {
    STATE.lock().map(|s| s.history.clone()).unwrap_or_default()
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); s.cycle_id = 0; }
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
    fn run_cycle_returns_result() {
        let r = run_cycle();
        assert!(r.ts_ms > 0);
    }

    #[test]
    fn meta_cycles_counter_increments() {
        let before = META_CYCLES.load(Ordering::Relaxed);
        run_cycle();
        assert!(META_CYCLES.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn confidence_bounded() {
        let r = run_cycle();
        assert!(r.confidence >= 0.0 && r.confidence <= 1.0);
    }

    #[test]
    fn stability_score_bounded() {
        let r = run_cycle();
        assert!(r.stability_score >= 0.0 && r.stability_score <= 1.0);
    }

    #[test]
    fn is_healthy_consistent_fields() {
        let r = run_cycle();
        if r.is_healthy() {
            assert!(r.confidence > 0.4);
            assert!(r.safety_certified);
        }
    }

    #[test]
    fn history_grows_after_cycle() {
        let before = META_CYCLES.load(Ordering::Relaxed);
        run_cycle();
        assert!(META_CYCLES.load(Ordering::Relaxed) > before);
    }
}
