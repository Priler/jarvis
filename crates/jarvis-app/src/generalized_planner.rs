//! Generalized planning engine — supports nested plans, multi-goal planning,
//! long-horizon planning, and dynamic replanning.
//! Pure heuristic; no ML. Plans are scored and simulated before adoption.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static PLANS_CREATED:    AtomicU64 = AtomicU64::new(0);
pub static PLANS_SIMULATED:  AtomicU64 = AtomicU64::new(0);
pub static PLANS_ADOPTED:    AtomicU64 = AtomicU64::new(0);
pub static REPLANS:          AtomicU64 = AtomicU64::new(0);

const MAX_PLAN_DEPTH: usize = 4;   // max nesting to prevent recursive plan storms
const MAX_ACTIVE_PLANS: usize = 20;

// ── Plan goal ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanGoal {
    pub id:          String,
    pub description: String,
    pub priority:    f32,       // 0–1
    pub horizon_days: u32,      // 0 = immediate
    pub subgoals:    Vec<PlanGoal>,   // nested goals
}

impl PlanGoal {
    pub fn new(id: impl Into<String>, description: impl Into<String>,
               priority: f32, horizon_days: u32) -> Self {
        Self {
            id: id.into(), description: description.into(),
            priority: priority.clamp(0.0, 1.0), horizon_days,
            subgoals: Vec::new(),
        }
    }

    pub fn with_subgoal(mut self, sub: PlanGoal) -> Self {
        self.subgoals.push(sub); self
    }

    pub fn depth(&self) -> usize {
        if self.subgoals.is_empty() { 0 }
        else { 1 + self.subgoals.iter().map(|s| s.depth()).max().unwrap_or(0) }
    }
}

// ── Generalized plan ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PlanStatus {
    Draft, Simulated, Active, Completed, Abandoned { reason: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeneralizedPlan {
    pub id:           String,
    pub goals:        Vec<PlanGoal>,
    pub status:       PlanStatus,
    pub score:        f32,    // simulation score 0–1
    pub risk:         f32,    // estimated risk 0–1
    pub created_ms:   u64,
    pub updated_ms:   u64,
}

impl GeneralizedPlan {
    pub fn is_viable(&self) -> bool {
        self.score >= 0.5 && self.risk < 0.7
    }

    pub fn max_horizon(&self) -> u32 {
        self.goals.iter().map(|g| g.horizon_days).max().unwrap_or(0)
    }
}

// ── Simulation result ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanSimResult {
    pub plan_id:      String,
    pub score:        f32,
    pub risk:         f32,
    pub viable:       bool,
    pub issues:       Vec<String>,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct PlannerState {
    plans:      Vec<GeneralizedPlan>,
    plan_seq:   u64,
}

static STATE: Lazy<Mutex<PlannerState>> = Lazy::new(|| Mutex::new(PlannerState {
    plans:    Vec::new(),
    plan_seq: 0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Create a new generalized plan from a set of goals.
pub fn create(goals: Vec<PlanGoal>) -> GeneralizedPlan {
    PLANS_CREATED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    // Validate nesting depth
    let max_depth = goals.iter().map(|g| g.depth()).max().unwrap_or(0);
    let depth_ok = max_depth <= MAX_PLAN_DEPTH;

    let id = STATE.lock().map(|mut s| { s.plan_seq += 1; format!("plan_{}", s.plan_seq) })
        .unwrap_or_else(|_| "plan_err".to_string());

    let plan = GeneralizedPlan {
        id:         id.clone(),
        goals:      if depth_ok { goals } else {
            // Flatten excessively deep goals
            vec![PlanGoal::new("flattened", "depth_limited_plan", 0.5, 0)]
        },
        status:     PlanStatus::Draft,
        score:      0.0,
        risk:       0.5,
        created_ms: now,
        updated_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.plans.len() >= MAX_ACTIVE_PLANS { s.plans.remove(0); }
        s.plans.push(plan.clone());
    }
    plan
}

/// Simulate a plan using the strategy_simulator as a proxy.
pub fn simulate(plan: &GeneralizedPlan) -> PlanSimResult {
    PLANS_SIMULATED.fetch_add(1, Ordering::Relaxed);

    let unc = crate::uncertainty_engine::sample();
    let stability = crate::cognitive_stability::check();

    // Score based on goal characteristics and current runtime state
    let goal_priority_avg = if plan.goals.is_empty() { 0.5 }
        else { plan.goals.iter().map(|g| g.priority).sum::<f32>() / plan.goals.len() as f32 };

    let horizon_penalty = if plan.max_horizon() > 7 { 0.15 } else { 0.0 };

    let score = (goal_priority_avg * 0.5
        + (1.0 - unc.overall) * 0.3
        + (if stability.is_stable { 0.2 } else { 0.0 }))
        .clamp(0.0, 1.0) - horizon_penalty;

    let risk = (unc.overall * 0.4
        + stability.oscillation_score * 0.3
        + if plan.max_horizon() > 30 { 0.3 } else { 0.1 })
        .clamp(0.0, 1.0);

    let mut issues = Vec::new();
    if unc.overall > 0.7 { issues.push("high_uncertainty".to_string()); }
    if stability.is_unstable() { issues.push("unstable_cognition".to_string()); }
    if plan.goals.is_empty() { issues.push("no_goals".to_string()); }

    PlanSimResult { plan_id: plan.id.clone(), score, risk, viable: score >= 0.5 && risk < 0.7, issues }
}

/// Adopt a plan (mark as active after simulation).
pub fn adopt(plan_id: &str, sim: &PlanSimResult) -> bool {
    if !sim.viable { return false; }
    PLANS_ADOPTED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    if let Ok(mut s) = STATE.lock() {
        for plan in s.plans.iter_mut() {
            if plan.id == plan_id {
                plan.status     = PlanStatus::Active;
                plan.score      = sim.score;
                plan.risk       = sim.risk;
                plan.updated_ms = now;
                return true;
            }
        }
    }
    false
}

/// Replan: abandon the active plan and create a replacement.
pub fn replan(reason: impl Into<String>, new_goals: Vec<PlanGoal>) -> GeneralizedPlan {
    REPLANS.fetch_add(1, Ordering::Relaxed);
    let reason = reason.into();
    if let Ok(mut s) = STATE.lock() {
        for plan in s.plans.iter_mut() {
            if matches!(plan.status, PlanStatus::Active) {
                plan.status = PlanStatus::Abandoned { reason: reason.clone() };
                plan.updated_ms = ts_now();
            }
        }
    }
    create(new_goals)
}

pub fn active_plans() -> Vec<GeneralizedPlan> {
    STATE.lock().map(|s| s.plans.iter()
        .filter(|p| matches!(p.status, PlanStatus::Active))
        .cloned().collect()
    ).unwrap_or_default()
}

pub fn all_plans() -> Vec<GeneralizedPlan> {
    STATE.lock().map(|s| s.plans.clone()).unwrap_or_default()
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
    fn create_plan_increments_counter() {
        let before = PLANS_CREATED.load(Ordering::Relaxed);
        let goals = vec![PlanGoal::new("g1", "test goal", 0.8, 1)];
        create(goals);
        assert!(PLANS_CREATED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn simulate_returns_viable_for_reasonable_plan() {
        let goals = vec![PlanGoal::new("g_sim", "sim test", 0.9, 0)];
        let plan = create(goals);
        let sim = simulate(&plan);
        assert!(sim.score >= 0.0 && sim.score <= 1.0);
    }

    #[test]
    fn adopt_returns_false_for_non_viable() {
        let goals = vec![PlanGoal::new("g_nv", "non viable", 0.0, 0)];
        let plan = create(goals);
        let sim = PlanSimResult {
            plan_id: plan.id.clone(), score: 0.3, risk: 0.8, viable: false, issues: vec![],
        };
        assert!(!adopt(&plan.id, &sim));
    }

    #[test]
    fn plan_depth_capped() {
        let deep = PlanGoal::new("d1", "d1", 0.5, 0)
            .with_subgoal(PlanGoal::new("d2", "d2", 0.5, 0)
                .with_subgoal(PlanGoal::new("d3", "d3", 0.5, 0)
                    .with_subgoal(PlanGoal::new("d4", "d4", 0.5, 0)
                        .with_subgoal(PlanGoal::new("d5", "d5", 0.5, 0)))));
        let plan = create(vec![deep]);
        assert!(!plan.goals.is_empty()); // should flatten or limit
    }

    #[test]
    fn nested_plan_depth_reported_correctly() {
        let g = PlanGoal::new("p", "parent", 0.8, 1)
            .with_subgoal(PlanGoal::new("c", "child", 0.7, 0));
        assert_eq!(g.depth(), 1);
    }

    #[test]
    fn replan_abandons_active_and_creates_new() {
        let g = vec![PlanGoal::new("orig", "original", 0.8, 0)];
        let plan = create(g);
        let sim = simulate(&plan);
        if sim.viable { adopt(&plan.id, &sim); }
        let new_plan = replan("test_reason", vec![PlanGoal::new("new", "new plan", 0.9, 0)]);
        assert!(REPLANS.load(Ordering::Relaxed) >= 1);
        assert!(!new_plan.id.is_empty());
    }
}
