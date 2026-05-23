//! Abstract resource reasoner — reasons about conceptual bottlenecks,
//! generalized instability, conceptual load, and strategic fragility.
//! Uses existing AtomicU64 counters and local runtime signals only.

use std::sync::Mutex;
use once_cell::sync::Lazy;

// ── BottleneckKind ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BottleneckKind {
    CognitionSaturation,
    PlannerContention,
    MemoryPressure,
    StrategicFragility,
    ConceptualOverload,
    None,
}

// ── AbstractResourceSnapshot ──────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AbstractResourceSnapshot {
    pub cognition_load:      f32,    // 0–1
    pub planner_contention:  f32,    // 0–1
    pub memory_pressure:     f32,    // 0–1
    pub strategic_fragility: f32,    // 0–1
    pub conceptual_load:     f32,    // 0–1
    pub overall:             f32,    // weighted composite
    pub primary_bottleneck:  BottleneckKind,
    pub ts_ms:               u64,
}

impl AbstractResourceSnapshot {
    pub fn is_overloaded(&self) -> bool { self.overall > 0.75 }
    pub fn is_critical(&self)   -> bool { self.overall > 0.90 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ReasonerState {
    history: Vec<AbstractResourceSnapshot>,
}

static STATE: Lazy<Mutex<ReasonerState>> = Lazy::new(|| Mutex::new(ReasonerState {
    history: Vec::new(),
}));

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Sampling ──────────────────────────────────────────────────────────────────

pub fn sample() -> AbstractResourceSnapshot {
    // Gather signals from existing modules
    let concrete = crate::resource_reasoner::sample();
    let unc      = crate::uncertainty_engine::sample();
    let stability = crate::cognitive_stability::check();

    // Abstract dimensions
    let cognition_load     = concrete.readings.iter()
        .find(|p| matches!(p.resource, crate::resource_reasoner::ResourceKind::CognitionLoad))
        .map(|p| p.pressure)
        .unwrap_or(0.0);

    let planner_contention = {
        let created  = crate::generalized_planner::PLANS_CREATED.load(std::sync::atomic::Ordering::Relaxed);
        let adopted  = crate::generalized_planner::PLANS_ADOPTED.load(std::sync::atomic::Ordering::Relaxed);
        let replans  = crate::generalized_planner::REPLANS.load(std::sync::atomic::Ordering::Relaxed);
        let total = created + 1;
        let failure_rate = 1.0 - (adopted as f32 / total as f32).min(1.0);
        (failure_rate * 0.5 + (replans as f32 / (total as f32 + 1.0)).min(0.5)).min(1.0)
    };

    let memory_pressure = (crate::concept_engine::snapshot().len() as f32 / 500.0).min(1.0);

    let world = crate::conceptual_world_model::latest();
    let strategic_fragility = world.map(|w| w.strategic_fragility).unwrap_or(unc.overall * 0.6);

    let concept_count   = crate::concept_engine::reliable_concepts().len();
    let conceptual_load = (concept_count as f32 / 50.0).min(1.0)
        + stability.oscillation_score * 0.2;
    let conceptual_load = conceptual_load.min(1.0);

    // Weighted composite: cognition most critical
    let overall = (cognition_load    * 0.30
                 + planner_contention * 0.20
                 + memory_pressure    * 0.15
                 + strategic_fragility * 0.20
                 + conceptual_load    * 0.15)
        .clamp(0.0, 1.0);

    // Primary bottleneck = whichever dimension is highest
    let dims = [
        (cognition_load,     BottleneckKind::CognitionSaturation),
        (planner_contention, BottleneckKind::PlannerContention),
        (memory_pressure,    BottleneckKind::MemoryPressure),
        (strategic_fragility, BottleneckKind::StrategicFragility),
        (conceptual_load,    BottleneckKind::ConceptualOverload),
    ];
    let primary_bottleneck = dims.iter()
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(v, k)| if *v > 0.5 { k.clone() } else { BottleneckKind::None })
        .unwrap_or(BottleneckKind::None);

    let snap = AbstractResourceSnapshot {
        cognition_load, planner_contention, memory_pressure,
        strategic_fragility, conceptual_load, overall, primary_bottleneck,
        ts_ms: ts_now(),
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(snap.clone());
    }
    snap
}

pub fn latest() -> Option<AbstractResourceSnapshot> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn primary_bottleneck() -> BottleneckKind {
    latest().map(|s| s.primary_bottleneck).unwrap_or(BottleneckKind::None)
}
