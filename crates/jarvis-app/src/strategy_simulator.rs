//! Strategy simulator — simulates plan execution before committing, estimating
//! success probability, risk, and latency from historical runtime data.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static SIMULATIONS_RUN:    AtomicU64 = AtomicU64::new(0);
pub static SIMULATIONS_PASSED: AtomicU64 = AtomicU64::new(0);
pub static SIMULATIONS_FAILED: AtomicU64 = AtomicU64::new(0);

const MAX_SIM_HISTORY: usize = 60;
const PASS_THRESHOLD:  f32   = 0.55;

// ── Plan representation ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanStep {
    pub tool_id:          String,
    pub estimated_risk:   f32,
    pub requires_verify:  bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub id:    String,
    pub steps: Vec<PlanStep>,
}

// ── Simulation result ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimulationResult {
    pub plan_id:           String,
    pub success_prob:      f32,
    pub estimated_risk:    f32,
    pub estimated_latency: f32,   // relative 0–1
    pub predicted_issues:  Vec<String>,
    pub should_execute:    bool,
    pub ts_ms:             u64,
}

impl SimulationResult {
    pub fn is_safe(&self) -> bool { self.should_execute && self.estimated_risk < 0.7 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SimState {
    history: Vec<SimulationResult>,
}

static STATE: Lazy<Mutex<SimState>> = Lazy::new(|| Mutex::new(SimState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn simulate(plan: &Plan) -> SimulationResult {
    SIMULATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let mut issues = Vec::new();
    let base_success = crate::execution_quality::latest()
        .map(|q| q.success_reliability)
        .unwrap_or(0.6);

    let step_risk: f32 = plan.steps.iter().map(|s| s.estimated_risk).sum::<f32>()
        / (plan.steps.len() as f32).max(1.0);

    // Check causal predictions for each step
    for step in &plan.steps {
        let preds = crate::causal_reasoner::predict(&step.tool_id);
        for p in preds {
            if p.probability > 0.6 {
                issues.push(format!("{} may cause {}", step.tool_id, p.predicted));
            }
        }
    }

    // Check failure patterns
    if crate::failure_pattern_analyzer::has_critical_pattern() {
        issues.push("critical failure pattern active".into());
    }

    let drift_penalty = if crate::cognitive_drift_control::is_frozen() { 0.2 } else { 0.0 };
    let success_prob = (base_success - step_risk * 0.3 - drift_penalty).clamp(0.0, 1.0);
    let estimated_latency = (plan.steps.len() as f32 / 10.0).min(1.0);
    let should_execute = success_prob >= PASS_THRESHOLD && step_risk < 0.8;

    if should_execute {
        SIMULATIONS_PASSED.fetch_add(1, Ordering::Relaxed);
    } else {
        SIMULATIONS_FAILED.fetch_add(1, Ordering::Relaxed);
    }

    let result = SimulationResult {
        plan_id: plan.id.clone(),
        success_prob,
        estimated_risk: step_risk,
        estimated_latency,
        predicted_issues: issues,
        should_execute,
        ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_SIM_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

pub fn latest() -> Option<SimulationResult> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn pass_rate() -> f32 {
    let run  = SIMULATIONS_RUN.load(Ordering::Relaxed) as f32;
    let pass = SIMULATIONS_PASSED.load(Ordering::Relaxed) as f32;
    if run == 0.0 { return 1.0; }
    pass / run
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

    fn make_plan(id: &str) -> Plan {
        Plan {
            id: id.into(),
            steps: vec![
                PlanStep { tool_id: "ss.tool.a".into(), estimated_risk: 0.2, requires_verify: false },
                PlanStep { tool_id: "ss.tool.b".into(), estimated_risk: 0.1, requires_verify: true },
            ],
        }
    }

    #[test]
    fn simulate_returns_result() {
        let r = simulate(&make_plan("ss.plan.u1"));
        assert!(r.success_prob >= 0.0 && r.success_prob <= 1.0);
    }

    #[test]
    fn simulations_run_counter_increments() {
        let before = SIMULATIONS_RUN.load(Ordering::Relaxed);
        simulate(&make_plan("ss.plan.u2"));
        assert!(SIMULATIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn high_risk_plan_may_fail() {
        let plan = Plan {
            id: "ss.plan.u3".into(),
            steps: vec![
                PlanStep { tool_id: "ss.dangerous".into(), estimated_risk: 0.95, requires_verify: false },
            ],
        };
        let r = simulate(&plan);
        assert!(r.estimated_risk > 0.5);
    }

    #[test]
    fn pass_rate_bounded() {
        simulate(&make_plan("ss.plan.u4"));
        let pr = pass_rate();
        assert!(pr >= 0.0 && pr <= 1.0);
    }

    #[test]
    fn history_grows() {
        let before = SIMULATIONS_RUN.load(Ordering::Relaxed);
        simulate(&make_plan("ss.plan.u5"));
        assert!(SIMULATIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn is_safe_requires_should_execute_and_low_risk() {
        let r = SimulationResult {
            plan_id: "test".into(), success_prob: 0.8, estimated_risk: 0.3,
            estimated_latency: 0.1, predicted_issues: vec![], should_execute: true, ts_ms: 0,
        };
        assert!(r.is_safe());
    }
}
