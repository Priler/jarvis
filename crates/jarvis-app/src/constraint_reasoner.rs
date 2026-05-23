//! Constraint reasoning engine — checks resource, planner, causal, workflow,
//! semantic, and stability constraints from live runtime signals.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

// ── ConstraintKind ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ConstraintKind {
    ResourceConstraint,
    PlannerConstraint,
    CausalConstraint,
    WorkflowConstraint,
    SemanticConstraint,
    StabilityConstraint,
}

// ── Constraint ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Constraint {
    pub kind:        ConstraintKind,
    pub description: String,
    pub violated:    bool,
    pub severity:    f32,    // 0–1
}

// ── ConstraintReport ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConstraintReport {
    pub constraints:      Vec<Constraint>,
    pub violated_count:   usize,
    pub max_severity:     f32,
    pub is_feasible:      bool,    // true if no critical violations
    pub ts_ms:            u64,
}

impl ConstraintReport {
    pub fn has_critical_violation(&self) -> bool { self.max_severity > 0.8 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ConstraintState {
    history: Vec<ConstraintReport>,
}

static STATE: Lazy<Mutex<ConstraintState>> = Lazy::new(|| Mutex::new(ConstraintState {
    history: Vec::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Constraint checks ─────────────────────────────────────────────────────────

pub fn check_constraints() -> ConstraintReport {
    let unc      = crate::uncertainty_engine::sample();
    let stability = crate::cognitive_stability::check();
    let emergency = crate::resource_scheduler::is_emergency();
    let abs_res   = crate::abstract_resource_reasoner::sample();
    let causal_links = crate::causal_reasoner::reliable_links();
    let active_plans = crate::generalized_planner::active_plans();
    let contradictions = crate::semantic_contradictions::active_contradictions();

    let mut constraints = Vec::new();

    // 1. Resource constraint
    let resource_sev = abs_res.overall;
    constraints.push(Constraint {
        kind:        ConstraintKind::ResourceConstraint,
        description: format!("abstract_resource_pressure:{:.2}", resource_sev),
        violated:    emergency || resource_sev > 0.75,
        severity:    resource_sev,
    });

    // 2. Planner constraint
    let plan_sev = if active_plans.is_empty() { 0.0 } else {
        active_plans.iter().map(|p| p.risk).fold(0.0f32, f32::max)
    };
    constraints.push(Constraint {
        kind:        ConstraintKind::PlannerConstraint,
        description: format!("max_plan_risk:{:.2}", plan_sev),
        violated:    plan_sev > 0.7,
        severity:    plan_sev,
    });

    // 3. Causal constraint (too many unstable causal links)
    let unstable_causal = causal_links.iter().filter(|l| !l.is_stable).count();
    let causal_sev = (unstable_causal as f32 / 10.0).min(1.0);
    constraints.push(Constraint {
        kind:        ConstraintKind::CausalConstraint,
        description: format!("unstable_causal_links:{}", unstable_causal),
        violated:    causal_sev > 0.5,
        severity:    causal_sev,
    });

    // 4. Workflow constraint (uncertainty too high for reliable execution)
    let workflow_sev = unc.overall;
    constraints.push(Constraint {
        kind:        ConstraintKind::WorkflowConstraint,
        description: format!("uncertainty:{:.2}", workflow_sev),
        violated:    workflow_sev > 0.8,
        severity:    workflow_sev,
    });

    // 5. Semantic constraint (active contradictions)
    let semantic_sev = (contradictions.len() as f32 / 5.0).min(1.0);
    constraints.push(Constraint {
        kind:        ConstraintKind::SemanticConstraint,
        description: format!("active_contradictions:{}", contradictions.len()),
        violated:    !contradictions.is_empty(),
        severity:    semantic_sev,
    });

    // 6. Stability constraint
    let stability_sev = stability.oscillation_score;
    constraints.push(Constraint {
        kind:        ConstraintKind::StabilityConstraint,
        description: format!("oscillation:{:.2}", stability_sev),
        violated:    !stability.is_stable,
        severity:    stability_sev,
    });

    // Log violations to symbolic observability
    for c in &constraints {
        if c.violated {
            crate::symbolic_observability::log(
                crate::symbolic_observability::SymbolicEvent::ConstraintViolated {
                    kind:     format!("{:?}", c.kind),
                    severity: c.severity,
                }
            );
        }
    }

    let violated_count = constraints.iter().filter(|c| c.violated).count();
    let max_severity = constraints.iter().map(|c| c.severity).fold(0.0f32, f32::max);
    let is_feasible = max_severity < 0.8;

    let report = ConstraintReport {
        constraints, violated_count, max_severity, is_feasible, ts_ms: ts_now(),
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(report.clone());
    }
    report
}

pub fn latest() -> Option<ConstraintReport> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}
