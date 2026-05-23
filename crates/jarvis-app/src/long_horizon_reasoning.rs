//! Long-horizon reasoning — multi-day goals, persistent workflows, strategic
//! environment evolution, long-term optimization.
//! Integrates with long_horizon_goals for persistence.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static LH_CYCLES:          AtomicU64 = AtomicU64::new(0);
pub static GOALS_PROGRESSED:   AtomicU64 = AtomicU64::new(0);
pub static GOALS_COMPLETED_LH: AtomicU64 = AtomicU64::new(0);
pub static STRATEGIC_UPDATES:  AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 50;

// ── Long-horizon assessment ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HorizonAssessment {
    pub cycle:          u64,
    pub active_goals:   usize,
    pub avg_progress:   f32,
    pub strategic_risk: f32,   // 0–1
    pub env_trajectory: EnvTrajectory,
    pub recommendation: String,
    pub ts_ms:          u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EnvTrajectory {
    Improving,
    Stable,
    Degrading { rate: f32 },
}

impl HorizonAssessment {
    pub fn needs_replan(&self) -> bool {
        self.strategic_risk >= 0.7 || matches!(self.env_trajectory, EnvTrajectory::Degrading { rate } if rate >= 0.5)
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct LHState {
    history:    Vec<HorizonAssessment>,
    cycle:      u64,
    prev_drift: f32,
}

static STATE: Lazy<Mutex<LHState>> = Lazy::new(|| Mutex::new(LHState {
    history:    Vec::new(),
    cycle:      0,
    prev_drift: 0.0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Run a long-horizon reasoning cycle.
pub fn reason() -> HorizonAssessment {
    LH_CYCLES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let cycle = STATE.lock().map(|mut s| { s.cycle += 1; s.cycle }).unwrap_or(0);

    let active = crate::long_horizon_goals::active_goals();
    let active_count = active.len();

    // Progress all active goals slightly (simulated continuous progress)
    for g in &active {
        let new_progress = (g.progress + 0.01).min(1.0);
        if new_progress >= 1.0 {
            crate::long_horizon_goals::complete(g.id);
            GOALS_COMPLETED_LH.fetch_add(1, Ordering::Relaxed);
        } else {
            crate::long_horizon_goals::update_progress(g.id, new_progress);
            GOALS_PROGRESSED.fetch_add(1, Ordering::Relaxed);
        }
    }

    let avg_progress = if active.is_empty() { 1.0 }
        else { active.iter().map(|g| g.progress).sum::<f32>() / active.len() as f32 };

    // Derive environment trajectory from uncertainty and stability
    let unc = crate::uncertainty_engine::sample();
    let stability = crate::cognitive_stability::check();

    let current_drift = unc.overall * 0.6 + stability.oscillation_score * 0.4;
    let env_trajectory = STATE.lock().map(|s| {
        let delta = current_drift - s.prev_drift;
        if delta < -0.05 { EnvTrajectory::Improving }
        else if delta > 0.10 { EnvTrajectory::Degrading { rate: delta } }
        else { EnvTrajectory::Stable }
    }).unwrap_or(EnvTrajectory::Stable);

    if let Ok(mut s) = STATE.lock() { s.prev_drift = current_drift; }

    let strategic_risk = (current_drift * 0.5
        + if active_count == 0 { 0.3 } else { 0.0 }
        + if avg_progress < 0.2 && active_count > 0 { 0.2 } else { 0.0 })
        .clamp(0.0, 1.0);

    let recommendation = if strategic_risk >= 0.7 {
        "replan_required:high_strategic_risk".to_string()
    } else if matches!(env_trajectory, EnvTrajectory::Degrading { .. }) {
        "monitor_env_degradation".to_string()
    } else {
        "continue_current_plans".to_string()
    };

    // Record causal relationships
    if current_drift > 0.5 {
        crate::causal_reasoner::observe("env_degradation", "strategic_risk_increase", current_drift);
        STRATEGIC_UPDATES.fetch_add(1, Ordering::Relaxed);
    }

    let assessment = HorizonAssessment {
        cycle, active_goals: active_count, avg_progress, strategic_risk,
        env_trajectory, recommendation, ts_ms: now,
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(assessment.clone());
    }

    assessment
}

/// Add a long-horizon goal from outside the layer system.
pub fn add_goal(description: impl Into<String>, horizon_days: u32) -> u64 {
    let kind = if horizon_days >= 7 {
        crate::long_horizon_goals::HorizonKind::MultiDayGoal {
            title:       description.into(),
            deadline_ms: None,
        }
    } else {
        crate::long_horizon_goals::HorizonKind::BackgroundObjective {
            description: description.into(),
        }
    };
    crate::long_horizon_goals::add(kind)
}

pub fn history(n: usize) -> Vec<HorizonAssessment> {
    STATE.lock().map(|s| s.history.iter().rev().take(n).cloned().collect()).unwrap_or_default()
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
    fn reason_returns_assessment() {
        let a = reason();
        assert!(LH_CYCLES.load(Ordering::Relaxed) >= 1);
        assert!(a.strategic_risk >= 0.0 && a.strategic_risk <= 1.0);
    }

    #[test]
    fn add_goal_creates_lh_goal() {
        let id = add_goal("test multi-day goal", 10);
        assert!(id > 0);
    }

    #[test]
    fn recommendation_non_empty() {
        let a = reason();
        assert!(!a.recommendation.is_empty());
    }

    #[test]
    fn multiple_cycles_increment_counter() {
        let before = LH_CYCLES.load(Ordering::Relaxed);
        reason();
        reason();
        assert!(LH_CYCLES.load(Ordering::Relaxed) >= before + 2);
    }
}
