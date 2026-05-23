//! Confidence reasoner — estimates reasoning, planner, and semantic confidence
//! from live runtime signals. Detects weak evidence chains and suppresses
//! unstable conclusions.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY:    usize = 100;
const MIN_ACCEPTABLE: f32   = 0.40;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ConfidenceReport ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConfidenceReport {
    pub reasoning_confidence: f32,
    pub planner_confidence:   f32,
    pub semantic_reliability: f32,
    pub resource_confidence:  f32,
    pub overall:              f32,
    pub weak_areas:           Vec<String>,
    pub ts_ms:                u64,
}

impl ConfidenceReport {
    pub fn has_weak_evidence(&self) -> bool {
        !self.weak_areas.is_empty() || self.overall < MIN_ACCEPTABLE
    }
    pub fn is_suppressed(&self) -> bool {
        self.overall < 0.25
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<ConfidenceReport>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Assessment ────────────────────────────────────────────────────────────────

pub fn assess() -> ConfidenceReport {
    let stability  = crate::semantic_stability::check();
    let cog_stab   = crate::cognitive_stability::check();
    let unc        = crate::uncertainty_engine::sample();
    let resource   = crate::abstract_resource_reasoner::sample();

    let semantic_reliability = (1.0 - stability.instability_score).clamp(0.0, 1.0);
    let reasoning_confidence = (1.0 - cog_stab.oscillation_score).clamp(0.0, 1.0);
    let planner_confidence   = (1.0 - unc.overall).clamp(0.0, 1.0);
    let resource_confidence  = (1.0 - resource.overall).clamp(0.0, 1.0);

    let overall = reasoning_confidence  * 0.30
                + semantic_reliability  * 0.30
                + planner_confidence    * 0.25
                + resource_confidence   * 0.15;

    let mut weak_areas: Vec<String> = Vec::new();
    if reasoning_confidence  < MIN_ACCEPTABLE { weak_areas.push("reasoning".into()); }
    if planner_confidence    < MIN_ACCEPTABLE { weak_areas.push("planner".into()); }
    if semantic_reliability  < MIN_ACCEPTABLE { weak_areas.push("semantic".into()); }
    if resource_confidence   < MIN_ACCEPTABLE { weak_areas.push("resource".into()); }

    let report = ConfidenceReport {
        reasoning_confidence,
        planner_confidence,
        semantic_reliability,
        resource_confidence,
        overall: overall.clamp(0.0, 1.0),
        weak_areas,
        ts_ms: ts_now(),
    };

    let mut h = HISTORY.lock().unwrap();
    if h.len() >= MAX_HISTORY { h.remove(0); }
    h.push(report.clone());
    report
}

pub fn latest() -> Option<ConfidenceReport> {
    HISTORY.lock().unwrap().last().cloned()
}

pub fn history(n: usize) -> Vec<ConfidenceReport> {
    let h = HISTORY.lock().unwrap();
    h.iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assess_valid_range() {
        let r = assess();
        assert!(r.overall >= 0.0 && r.overall <= 1.0);
        assert!(r.reasoning_confidence >= 0.0);
        assert!(r.semantic_reliability >= 0.0);
    }

    #[test]
    fn latest_after_assess() {
        assess();
        assert!(latest().is_some());
    }
}
