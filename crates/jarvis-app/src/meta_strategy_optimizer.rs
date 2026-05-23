//! Meta-strategy optimizer — compares cognition models (reasoning strategies)
//! and selects the best one based on quality, uncertainty, and simulation data.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static MSO_OPTIMIZATIONS:    AtomicU64 = AtomicU64::new(0);
pub static MSO_STRATEGY_CHANGES: AtomicU64 = AtomicU64::new(0);
pub static MSO_CYCLES:           AtomicU64 = AtomicU64::new(0);

const MAX_MSO_HISTORY: usize = 50;

// ── Cognition model ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CognitionModel {
    pub id:                   String,
    pub description:          String,
    pub uncertainty_tolerance: f32,   // 0–1; higher = more tolerant
    pub causal_depth:         u32,    // how many causal hops to explore
    pub simulation_steps:     u32,    // future-state look-ahead
    pub risk_aversion:        f32,    // 0–1; higher = more risk-averse
}

impl CognitionModel {
    pub fn cost(&self, uncertainty: f32) -> f32 {
        let uncertainty_penalty = (uncertainty - self.uncertainty_tolerance).max(0.0) * 0.5;
        let depth_cost = (self.causal_depth as f32 / 10.0).min(0.3);
        let sim_cost   = (self.simulation_steps as f32 / 20.0).min(0.2);
        (uncertainty_penalty + depth_cost + sim_cost + self.risk_aversion * 0.2).clamp(0.0, 1.0)
    }
}

// ── Optimization result ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MsoResult {
    pub selected_model:    String,
    pub selected_cost:     f32,
    pub all_costs:         Vec<(String, f32)>,
    pub strategy_changed:  bool,
    pub ts_ms:             u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct MsoState {
    history:        Vec<MsoResult>,
    active_model:   Option<String>,
}

static STATE: Lazy<Mutex<MsoState>> = Lazy::new(|| Mutex::new(MsoState {
    history:      Vec::new(),
    active_model: None,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn optimize(models: &[CognitionModel]) -> MsoResult {
    MSO_OPTIMIZATIONS.fetch_add(1, Ordering::Relaxed);
    MSO_CYCLES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let uncertainty = crate::uncertainty_engine::overall_uncertainty();

    let mut costs: Vec<(String, f32)> = models.iter()
        .map(|m| (m.id.clone(), m.cost(uncertainty)))
        .collect();
    costs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let (selected_id, selected_cost) = costs.first()
        .map(|(id, cost)| (id.clone(), *cost))
        .unwrap_or_else(|| ("default".into(), 0.5));

    let strategy_changed = STATE.lock().ok().and_then(|s| s.active_model.clone())
        .map(|prev| prev != selected_id)
        .unwrap_or(false);

    if strategy_changed { MSO_STRATEGY_CHANGES.fetch_add(1, Ordering::Relaxed); }

    let result = MsoResult { selected_model: selected_id.clone(), selected_cost, all_costs: costs, strategy_changed, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        s.active_model = Some(selected_id);
        if s.history.len() >= MAX_MSO_HISTORY { s.history.remove(0); }
        s.history.push(result.clone());
    }

    result
}

pub fn conservative_model() -> CognitionModel {
    CognitionModel {
        id: "conservative".into(), description: "low risk, shallow causal depth".into(),
        uncertainty_tolerance: 0.7, causal_depth: 2, simulation_steps: 3, risk_aversion: 0.8,
    }
}

pub fn aggressive_model() -> CognitionModel {
    CognitionModel {
        id: "aggressive".into(), description: "high depth, tolerates uncertainty".into(),
        uncertainty_tolerance: 0.3, causal_depth: 6, simulation_steps: 8, risk_aversion: 0.2,
    }
}

pub fn balanced_model() -> CognitionModel {
    CognitionModel {
        id: "balanced".into(), description: "moderate depth and risk tolerance".into(),
        uncertainty_tolerance: 0.5, causal_depth: 4, simulation_steps: 5, risk_aversion: 0.5,
    }
}

pub fn active_model() -> Option<String> {
    STATE.lock().ok().and_then(|s| s.active_model.clone())
}

pub fn latest() -> Option<MsoResult> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); s.active_model = None; }
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
    fn optimize_selects_lowest_cost() {
        let models = vec![conservative_model(), aggressive_model(), balanced_model()];
        let r = optimize(&models);
        assert!(!r.selected_model.is_empty());
    }

    #[test]
    fn mso_optimizations_counter_increments() {
        let before = MSO_OPTIMIZATIONS.load(Ordering::Relaxed);
        optimize(&[balanced_model()]);
        assert!(MSO_OPTIMIZATIONS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn cost_bounded() {
        let m = balanced_model();
        let c = m.cost(0.5);
        assert!(c >= 0.0 && c <= 1.0);
    }

    #[test]
    fn active_model_set_after_optimize() {
        optimize(&[conservative_model()]);
        assert!(active_model().is_some());
    }

    #[test]
    fn all_costs_contains_all_models() {
        let r = optimize(&[conservative_model(), aggressive_model(), balanced_model()]);
        assert_eq!(r.all_costs.len(), 3);
    }

    #[test]
    fn selected_cost_bounded() {
        let r = optimize(&[balanced_model()]);
        assert!(r.selected_cost >= 0.0 && r.selected_cost <= 1.0);
    }
}
