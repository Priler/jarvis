//! Cognitive evolution engine — evolves planning heuristics, workflow priorities,
//! recovery decisions, and reasoning preferences over time.
//!
//! NO uncontrolled self-modification. All changes are bounded deltas on `const`
//! base values; the runtime cannot move weights outside safe intervals.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static EVOLUTIONS_RUN:     AtomicU64 = AtomicU64::new(0);
pub static HEURISTICS_UPDATED: AtomicU64 = AtomicU64::new(0);

// ── Heuristic weight bounds ───────────────────────────────────────────────────

const WEIGHT_MIN: f32 = 0.10;
const WEIGHT_MAX: f32 = 0.90;
const DELTA_STEP: f32 = 0.02;   // max change per evolution tick

// ── Runtime heuristics ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CognitiveHeuristics {
    pub planner_risk_weight:      f32,
    pub recovery_aggressiveness:  f32,
    pub workflow_priority_bias:   f32,
    pub attention_sensitivity:    f32,
    pub verification_strictness:  f32,
    pub generation:               u32,
    pub ts_ms:                    u64,
}

impl Default for CognitiveHeuristics {
    fn default() -> Self {
        Self {
            planner_risk_weight:     0.50,
            recovery_aggressiveness: 0.50,
            workflow_priority_bias:  0.50,
            attention_sensitivity:   0.50,
            verification_strictness: 0.60,
            generation:              0,
            ts_ms:                   0,
        }
    }
}

impl CognitiveHeuristics {
    fn clamp_all(mut self) -> Self {
        self.planner_risk_weight     = self.planner_risk_weight.clamp(WEIGHT_MIN, WEIGHT_MAX);
        self.recovery_aggressiveness = self.recovery_aggressiveness.clamp(WEIGHT_MIN, WEIGHT_MAX);
        self.workflow_priority_bias  = self.workflow_priority_bias.clamp(WEIGHT_MIN, WEIGHT_MAX);
        self.attention_sensitivity   = self.attention_sensitivity.clamp(WEIGHT_MIN, WEIGHT_MAX);
        self.verification_strictness = self.verification_strictness.clamp(WEIGHT_MIN, WEIGHT_MAX);
        self
    }
}

// ── Evolution record ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvolutionRecord {
    pub generation: u32,
    pub trigger:    String,
    pub delta:      String,
    pub ts_ms:      u64,
}

const MAX_RECORDS: usize = 50;

struct EvolutionState {
    heuristics: CognitiveHeuristics,
    records:    Vec<EvolutionRecord>,
}

static STATE: Lazy<Mutex<EvolutionState>> = Lazy::new(|| Mutex::new(EvolutionState {
    heuristics: CognitiveHeuristics::default(),
    records:    Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Evolve heuristics one step based on current quality scores.
/// Returns whether any weight changed.
pub fn evolve() -> bool {
    EVOLUTIONS_RUN.fetch_add(1, Ordering::Relaxed);

    if crate::cognitive_drift_control::is_frozen() {
        return false;
    }

    let scores = crate::strategy_evaluator::all_latest();
    if scores.is_empty() { return false; }

    let mut changed = false;

    if let Ok(mut s) = STATE.lock() {
        let now = ts_now();
        let gen = s.heuristics.generation + 1;

        // Planner quality → adjust risk weight
        if let Some(sc) = scores.iter().find(|s| s.dimension == crate::strategy_evaluator::DIM_PLANNER) {
            let delta = if sc.score < 0.5 { DELTA_STEP } else { -DELTA_STEP * 0.5 };
            s.heuristics.planner_risk_weight += delta;
            if delta.abs() > 0.001 {
                push_record(&mut s.records, gen, "planner_quality", &format!("Δ={:.3}", delta), now);
                changed = true;
            }
        }

        // Recovery quality → adjust recovery aggressiveness
        if let Some(sc) = scores.iter().find(|s| s.dimension == crate::strategy_evaluator::DIM_RECOVERY) {
            let delta = if sc.score < 0.5 { DELTA_STEP } else { -DELTA_STEP * 0.5 };
            s.heuristics.recovery_aggressiveness += delta;
            if delta.abs() > 0.001 {
                push_record(&mut s.records, gen, "recovery_quality", &format!("Δ={:.3}", delta), now);
                changed = true;
            }
        }

        // Attention quality → adjust sensitivity
        if let Some(sc) = scores.iter().find(|s| s.dimension == crate::strategy_evaluator::DIM_ATTENTION) {
            let delta = if sc.score < 0.6 { DELTA_STEP } else { -DELTA_STEP * 0.3 };
            s.heuristics.attention_sensitivity += delta;
            if delta.abs() > 0.001 {
                push_record(&mut s.records, gen, "attention_quality", &format!("Δ={:.3}", delta), now);
                changed = true;
            }
        }

        s.heuristics = s.heuristics.clone().clamp_all();
        s.heuristics.generation = gen;
        s.heuristics.ts_ms = now;

        if changed {
            HEURISTICS_UPDATED.fetch_add(1, Ordering::Relaxed);
        }
    }

    changed
}

pub fn current() -> CognitiveHeuristics {
    STATE.lock().map(|s| s.heuristics.clone()).unwrap_or_default()
}

pub fn generation() -> u32 {
    STATE.lock().map(|s| s.heuristics.generation).unwrap_or(0)
}

pub fn recent_records(n: usize) -> Vec<EvolutionRecord> {
    STATE.lock().map(|s| {
        let len = s.records.len();
        s.records[len.saturating_sub(n)..].to_vec()
    }).unwrap_or_default()
}

pub fn reset() {
    if let Ok(mut s) = STATE.lock() {
        s.heuristics = CognitiveHeuristics::default();
        s.records.clear();
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn push_record(records: &mut Vec<EvolutionRecord>, gen: u32, trigger: &str, delta: &str, now: u64) {
    if records.len() >= MAX_RECORDS { records.remove(0); }
    records.push(EvolutionRecord {
        generation: gen,
        trigger: trigger.into(),
        delta: delta.into(),
        ts_ms: now,
    });
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
    fn default_heuristics_valid() {
        let h = CognitiveHeuristics::default();
        assert!(h.planner_risk_weight >= WEIGHT_MIN && h.planner_risk_weight <= WEIGHT_MAX);
        assert!(h.recovery_aggressiveness >= WEIGHT_MIN && h.recovery_aggressiveness <= WEIGHT_MAX);
    }

    #[test]
    fn clamp_enforces_bounds() {
        let mut h = CognitiveHeuristics::default();
        h.planner_risk_weight = 5.0;
        h = h.clamp_all();
        assert!(h.planner_risk_weight <= WEIGHT_MAX);
    }

    #[test]
    fn evolve_returns_bool() {
        crate::cognitive_drift_control::unfreeze_for_test();
        let _changed = evolve();
    }

    #[test]
    fn evolutions_run_increments() {
        let before = EVOLUTIONS_RUN.load(Ordering::Relaxed);
        evolve();
        assert!(EVOLUTIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn weights_stay_bounded_after_many_evolutions() {
        reset();
        crate::cognitive_drift_control::unfreeze_for_test();
        for _ in 0..30 { evolve(); }
        let h = current();
        assert!(h.planner_risk_weight >= WEIGHT_MIN && h.planner_risk_weight <= WEIGHT_MAX);
        assert!(h.attention_sensitivity >= WEIGHT_MIN && h.attention_sensitivity <= WEIGHT_MAX);
    }

    #[test]
    fn generation_is_non_negative() {
        let g = generation();
        assert!(g < u32::MAX);
    }
}
