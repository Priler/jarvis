//! Generalized uncertainty reasoner — aggregates uncertainty across all
//! cognitive dimensions: planner, workflow, OCR, semantic, causal, resource.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── UncertaintyProfile ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UncertaintyProfile {
    pub planner_uncertainty:  f32,
    pub workflow_uncertainty: f32,
    pub ocr_uncertainty:      f32,
    pub semantic_uncertainty: f32,
    pub causal_uncertainty:   f32,
    pub resource_uncertainty: f32,
    pub overall:              f32,
    pub ts_ms:                u64,
}

impl UncertaintyProfile {
    pub fn is_critical(&self) -> bool { self.overall > 0.75 }
    pub fn is_high(&self)     -> bool { self.overall > 0.55 }

    pub fn most_uncertain(&self) -> &'static str {
        let vals = [
            ("planner",  self.planner_uncertainty),
            ("workflow", self.workflow_uncertainty),
            ("ocr",      self.ocr_uncertainty),
            ("semantic", self.semantic_uncertainty),
            ("causal",   self.causal_uncertainty),
            ("resource", self.resource_uncertainty),
        ];
        vals.iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(n, _)| *n)
            .unwrap_or("unknown")
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<UncertaintyProfile>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Sampling ──────────────────────────────────────────────────────────────────

pub fn profile() -> UncertaintyProfile {
    let unc      = crate::uncertainty_engine::sample();
    let resource = crate::abstract_resource_reasoner::sample();
    let stability = crate::semantic_stability::check();
    let causal   = crate::causal_reasoner::reliable_links();

    // Planner uncertainty from the planner dimension
    let planner_uncertainty = unc.readings.iter()
        .find(|r| r.dimension == crate::uncertainty_engine::DIM_PLANNER)
        .map(|r| r.combined)
        .unwrap_or(unc.overall);

    // Workflow uncertainty from workflow dimension
    let workflow_uncertainty = unc.readings.iter()
        .find(|r| r.dimension == crate::uncertainty_engine::DIM_WORKFLOW)
        .map(|r| r.combined)
        .unwrap_or(unc.overall);

    // OCR uncertainty — proxy from resource memory_pressure
    let ocr_uncertainty = resource.memory_pressure;

    // Semantic uncertainty from semantic instability
    let semantic_uncertainty = stability.instability_score;

    // Causal uncertainty: inverse of mean reliable link strength
    let causal_uncertainty = if causal.is_empty() {
        0.50
    } else {
        let mean: f32 = causal.iter().map(|l| l.strength).sum::<f32>() / causal.len() as f32;
        (1.0 - mean).clamp(0.0, 1.0)
    };

    let resource_uncertainty = resource.overall;

    let overall = (planner_uncertainty  * 0.20
        + workflow_uncertainty           * 0.20
        + semantic_uncertainty           * 0.20
        + causal_uncertainty             * 0.15
        + resource_uncertainty           * 0.15
        + ocr_uncertainty                * 0.10)
        .clamp(0.0, 1.0);

    let p = UncertaintyProfile {
        planner_uncertainty,
        workflow_uncertainty,
        ocr_uncertainty,
        semantic_uncertainty,
        causal_uncertainty,
        resource_uncertainty,
        overall,
        ts_ms: ts_now(),
    };

    let mut h = HISTORY.lock().unwrap();
    if h.len() >= MAX_HISTORY { h.remove(0); }
    h.push(p.clone());
    p
}

pub fn latest() -> Option<UncertaintyProfile> {
    HISTORY.lock().unwrap().last().cloned()
}

pub fn history(n: usize) -> Vec<UncertaintyProfile> {
    let h = HISTORY.lock().unwrap();
    h.iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_valid_range() {
        let p = profile();
        assert!(p.overall >= 0.0 && p.overall <= 1.0);
        assert!(p.planner_uncertainty >= 0.0);
        assert!(p.semantic_uncertainty >= 0.0);
    }

    #[test]
    fn most_uncertain_returns_str() {
        let p = profile();
        let _ = p.most_uncertain();
    }
}
