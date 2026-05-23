//! Adaptive topology — tracks cognition path loads, detects overloaded paths,
//! rebalances routing weights, and recommends optimal cognition paths.

use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub const OVERLOAD_THRESHOLD: f32 = 0.75;
pub const STABLE_THRESHOLD:   f32 = 0.45;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── CognitionPath ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CognitionPath {
    Symbolic,
    Probabilistic,
    Conceptual,
    Hierarchical,
    MetaCognition,
    Lightweight,
}

impl CognitionPath {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Symbolic       => "symbolic",
            Self::Probabilistic  => "probabilistic",
            Self::Conceptual     => "conceptual",
            Self::Hierarchical   => "hierarchical",
            Self::MetaCognition  => "meta_cognition",
            Self::Lightweight    => "lightweight",
        }
    }
    pub fn all() -> &'static [CognitionPath] {
        &[
            Self::Symbolic, Self::Probabilistic, Self::Conceptual,
            Self::Hierarchical, Self::MetaCognition, Self::Lightweight,
        ]
    }
}

// ── PathLoad ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PathLoad {
    pub path:            CognitionPath,
    pub load_score:      f32,       // 0–1, higher = more loaded
    pub is_stable:       bool,
    pub is_overloaded:   bool,
    pub routing_weight:  f32,       // 0–1, routing preference weight
    pub suppressed:      bool,
    pub updated_ms:      u64,
}

impl PathLoad {
    fn new(path: CognitionPath) -> Self {
        PathLoad {
            path,
            load_score:     0.20,
            is_stable:      true,
            is_overloaded:  false,
            routing_weight: 1.0 / 6.0,
            suppressed:     false,
            updated_ms:     0,
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct TopologyState {
    loads: HashMap<CognitionPath, PathLoad>,
}

impl TopologyState {
    fn new() -> Self {
        let mut loads = HashMap::new();
        for path in CognitionPath::all() {
            loads.insert(*path, PathLoad::new(*path));
        }
        TopologyState { loads }
    }
}

static STATE: Lazy<Mutex<TopologyState>> = Lazy::new(|| Mutex::new(TopologyState::new()));

// ── API ───────────────────────────────────────────────────────────────────────

/// Sample live runtime signals and refresh all path loads.
pub fn refresh_loads() {
    let stability    = crate::semantic_stability::check();
    let cog          = crate::cognitive_stability::check();
    let unc          = crate::uncertainty_engine::sample();
    let resource     = crate::abstract_resource_reasoner::sample();
    let prob_stab    = crate::probabilistic_stability::check();

    let mut s = STATE.lock().unwrap();
    let now   = ts_now();

    let updates = [
        (CognitionPath::Symbolic,      stability.instability_score),
        (CognitionPath::Probabilistic, prob_stab.instability_score),
        (CognitionPath::Conceptual,    resource.conceptual_load),
        (CognitionPath::Hierarchical,  cog.oscillation_score),
        (CognitionPath::MetaCognition, unc.overall),
        (CognitionPath::Lightweight,   0.15_f32),  // always low cost
    ];

    for (path, raw_load) in updates {
        if let Some(pl) = s.loads.get_mut(&path) {
            if pl.suppressed { continue; }
            // EMA update
            pl.load_score    = (pl.load_score * 0.70 + raw_load * 0.30).clamp(0.0, 1.0);
            pl.is_overloaded = pl.load_score > OVERLOAD_THRESHOLD;
            pl.is_stable     = pl.load_score < STABLE_THRESHOLD;
            pl.updated_ms    = now;
        }
    }
}

/// Rebalance routing weights: overloaded paths get reduced weight; stable paths increased.
pub fn rebalance() {
    let mut s = STATE.lock().unwrap();

    // Compute raw inverse-load weights
    let mut raw: HashMap<CognitionPath, f32> = HashMap::new();
    for (path, pl) in &s.loads {
        if pl.suppressed {
            raw.insert(*path, 0.0);
        } else {
            raw.insert(*path, (1.0 - pl.load_score).max(0.05));
        }
    }

    let total: f32 = raw.values().sum();
    if total <= 0.0 { return; }

    for (path, pl) in s.loads.iter_mut() {
        pl.routing_weight = raw[path] / total;
    }
}

/// Update a single path's load directly (from external observation).
pub fn update_load(path: CognitionPath, load: f32) {
    let mut s = STATE.lock().unwrap();
    if let Some(pl) = s.loads.get_mut(&path) {
        if !pl.suppressed {
            pl.load_score    = (pl.load_score * 0.60 + load.clamp(0.0,1.0) * 0.40).clamp(0.0, 1.0);
            pl.is_overloaded = pl.load_score > OVERLOAD_THRESHOLD;
            pl.is_stable     = pl.load_score < STABLE_THRESHOLD;
            pl.updated_ms    = ts_now();
        }
    }
}

pub fn suppress_path(path: CognitionPath) {
    let mut s = STATE.lock().unwrap();
    if let Some(pl) = s.loads.get_mut(&path) {
        pl.suppressed      = true;
        pl.routing_weight  = 0.0;
    }
}

pub fn restore_path(path: CognitionPath) {
    let mut s = STATE.lock().unwrap();
    if let Some(pl) = s.loads.get_mut(&path) {
        pl.suppressed      = false;
        pl.load_score      = 0.40;    // reset to moderate on restore
        pl.is_overloaded   = false;
        pl.is_stable       = true;
        pl.routing_weight  = 1.0 / 6.0;
        pl.updated_ms      = ts_now();
    }
}

pub fn all_loads() -> Vec<PathLoad> {
    STATE.lock().unwrap().loads.values().cloned().collect()
}

pub fn get_load(path: CognitionPath) -> f32 {
    STATE.lock().unwrap().loads.get(&path).map(|p| p.load_score).unwrap_or(0.5)
}

pub fn get_weight(path: CognitionPath) -> f32 {
    STATE.lock().unwrap().loads.get(&path).map(|p| p.routing_weight).unwrap_or(1.0/6.0)
}

pub fn overloaded_paths() -> Vec<CognitionPath> {
    STATE.lock().unwrap().loads.values()
        .filter(|p| p.is_overloaded && !p.suppressed)
        .map(|p| p.path)
        .collect()
}

pub fn suppressed_paths() -> Vec<CognitionPath> {
    STATE.lock().unwrap().loads.values()
        .filter(|p| p.suppressed)
        .map(|p| p.path)
        .collect()
}

/// Path with lowest load that is not suppressed and has positive weight.
pub fn recommended_path() -> CognitionPath {
    let s = STATE.lock().unwrap();
    s.loads.values()
        .filter(|p| !p.suppressed && p.routing_weight > 0.0)
        .min_by(|a, b| a.load_score.partial_cmp(&b.load_score).unwrap())
        .map(|p| p.path)
        .unwrap_or(CognitionPath::Lightweight)
}

pub fn avg_load() -> f32 {
    let s = STATE.lock().unwrap();
    let active: Vec<f32> = s.loads.values()
        .filter(|p| !p.suppressed)
        .map(|p| p.load_score)
        .collect();
    if active.is_empty() { return 0.5; }
    active.iter().sum::<f32>() / active.len() as f32
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_and_rebalance_no_panic() {
        refresh_loads();
        rebalance();
        let loads = all_loads();
        assert_eq!(loads.len(), 6);
    }

    #[test]
    fn suppress_and_restore() {
        suppress_path(CognitionPath::Lightweight);
        assert!(suppressed_paths().contains(&CognitionPath::Lightweight));
        restore_path(CognitionPath::Lightweight);
        assert!(!suppressed_paths().contains(&CognitionPath::Lightweight));
    }

    #[test]
    fn recommended_path_returns_valid() {
        refresh_loads();
        let p = recommended_path();
        assert!(CognitionPath::all().contains(&p));
    }
}
