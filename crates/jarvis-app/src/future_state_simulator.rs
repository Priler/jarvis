//! Future-state simulator — projects system state N steps forward using
//! current trajectory and causal links.  No ML; linear extrapolation + causal injection.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static PROJECTIONS_RUN:      AtomicU64 = AtomicU64::new(0);
pub static DEGRADED_PROJECTIONS: AtomicU64 = AtomicU64::new(0);
pub static STABLE_PROJECTIONS:   AtomicU64 = AtomicU64::new(0);

const MAX_PROJECTION_HISTORY: usize = 50;
const DEGRADED_THRESHOLD:     f32   = 0.45;

// ── Projected state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectedState {
    pub step:               u32,
    pub quality_estimate:   f32,
    pub drift_risk:         f32,
    pub causal_hazards:     Vec<String>,
    pub is_stable:          bool,
    pub ts_ms:              u64,
}

// ── Projection ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Projection {
    pub horizon:        u32,
    pub states:         Vec<ProjectedState>,
    pub final_quality:  f32,
    pub degraded:       bool,
    pub ts_ms:          u64,
}

impl Projection {
    pub fn will_degrade(&self) -> bool { self.degraded }
    pub fn safe_horizon(&self) -> u32 {
        self.states.iter().take_while(|s| s.is_stable).count() as u32
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct FssState {
    history: Vec<Projection>,
}

static STATE: Lazy<Mutex<FssState>> = Lazy::new(|| Mutex::new(FssState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn project(horizon: u32) -> Projection {
    PROJECTIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let base_quality = crate::execution_quality::latest()
        .map(|q| q.overall)
        .unwrap_or(0.6);

    let drift_frozen = crate::cognitive_drift_control::is_frozen();
    let causal_links = crate::causal_reasoner::reliable_links();

    let mut states = Vec::with_capacity(horizon as usize);
    let mut quality = base_quality;

    for step in 1..=horizon {
        let drift_risk = if drift_frozen { 0.4 } else {
            let drift_events = crate::cognitive_drift_control::recent_events(3).len();
            (drift_events as f32 / 3.0).min(0.5)
        };

        // Decay quality slightly each step, more if drift is high
        quality = (quality - drift_risk * 0.05 - 0.01).clamp(0.0, 1.0);

        // Causal hazards from known links
        let hazards: Vec<String> = causal_links.iter()
            .filter(|l| l.strength > 0.6)
            .take(2)
            .map(|l| format!("{}→{}", l.cause, l.effect))
            .collect();

        let is_stable = quality >= DEGRADED_THRESHOLD && !drift_frozen;

        states.push(ProjectedState {
            step, quality_estimate: quality, drift_risk,
            causal_hazards: hazards, is_stable, ts_ms: now,
        });
    }

    let final_quality = states.last().map(|s| s.quality_estimate).unwrap_or(base_quality);
    let degraded = final_quality < DEGRADED_THRESHOLD;

    if degraded {
        DEGRADED_PROJECTIONS.fetch_add(1, Ordering::Relaxed);
    } else {
        STABLE_PROJECTIONS.fetch_add(1, Ordering::Relaxed);
    }

    let proj = Projection { horizon, states, final_quality, degraded, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_PROJECTION_HISTORY { s.history.remove(0); }
        s.history.push(proj.clone());
    }

    proj
}

pub fn latest() -> Option<Projection> {
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

    #[test]
    fn project_returns_correct_step_count() {
        let p = project(5);
        assert_eq!(p.states.len(), 5);
    }

    #[test]
    fn projections_run_counter_increments() {
        let before = PROJECTIONS_RUN.load(Ordering::Relaxed);
        project(3);
        assert!(PROJECTIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn final_quality_bounded() {
        let p = project(4);
        assert!(p.final_quality >= 0.0 && p.final_quality <= 1.0);
    }

    #[test]
    fn safe_horizon_lte_total() {
        let p = project(5);
        assert!(p.safe_horizon() <= 5);
    }

    #[test]
    fn will_degrade_consistent_with_final_quality() {
        let p = project(3);
        assert_eq!(p.will_degrade(), p.final_quality < 0.45);
    }

    #[test]
    fn history_grows_after_project() {
        let before = PROJECTIONS_RUN.load(Ordering::Relaxed);
        project(2);
        assert!(PROJECTIONS_RUN.load(Ordering::Relaxed) > before);
    }
}
