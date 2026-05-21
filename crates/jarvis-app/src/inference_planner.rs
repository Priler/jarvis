//! Inference planner — creates plans from symbolic inference chains.
//! Uses symbolic conclusions as goals, chain confidence as priority,
//! and constraint reports to filter infeasible plans.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static INFERENCE_PLANS_CREATED: AtomicU64 = AtomicU64::new(0);

const MAX_PLANS: usize = 50;

// ── InferencePlan ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferencePlan {
    pub id:                u64,
    pub source_chain_root: String,
    pub conclusion:        String,
    pub priority:          f32,
    pub feasible:          bool,
    pub generalized_plan_id: Option<String>,
    pub ts_ms:             u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct PlanState {
    plans: Vec<InferencePlan>,
    seq:   u64,
}

static STATE: Lazy<Mutex<PlanState>> = Lazy::new(|| Mutex::new(PlanState {
    plans: Vec::new(),
    seq:   0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Create an inference plan from a symbolic inference chain.
pub fn plan_from_inference(chain: &crate::symbolic_inference::InferenceChain)
    -> Option<InferencePlan>
{
    if !chain.is_reliable() { return None; }

    // Check constraint feasibility
    let constraint_report = crate::constraint_reasoner::latest();
    let feasible = constraint_report.map(|r| r.is_feasible).unwrap_or(true);

    // Create a generalized plan goal from the chain conclusion
    let goal = crate::generalized_planner::PlanGoal::new(
        format!("inferred_{}", chain.id),
        format!("resolve_{}", chain.conclusion),
        chain.confidence,
        0,    // immediate
    );

    // Use create_from_abstractions for enrichment
    let gen_plan = crate::generalized_planner::create_from_abstractions(vec![goal]);
    let gen_plan_id = if feasible {
        let sim = crate::generalized_planner::simulate(&gen_plan);
        crate::generalized_planner::adopt(&gen_plan.id, &sim);
        Some(gen_plan.id.clone())
    } else { None };

    INFERENCE_PLANS_CREATED.fetch_add(1, Ordering::Relaxed);

    let plan = if let Ok(mut s) = STATE.lock() {
        if s.plans.len() >= MAX_PLANS { s.plans.remove(0); }
        s.seq += 1;
        let id = s.seq;
        let p = InferencePlan {
            id,
            source_chain_root: chain.root.clone(),
            conclusion:        chain.conclusion.clone(),
            priority:          chain.confidence,
            feasible,
            generalized_plan_id: gen_plan_id,
            ts_ms: ts_now(),
        };
        s.plans.push(p.clone());
        p
    } else { return None; };

    Some(plan)
}

/// Create inference plans from all reliable chains.
pub fn plan_all_reliable() -> usize {
    let chains = crate::symbolic_inference::reliable_chains();
    let mut count = 0;
    for chain in chains.iter().take(5) {
        if plan_from_inference(chain).is_some() { count += 1; }
    }
    count
}

pub fn all_plans() -> Vec<InferencePlan> {
    STATE.lock().map(|s| s.plans.clone()).unwrap_or_default()
}
