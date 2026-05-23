//! Generalized observability — structured logging for the Phase 19 hierarchical
//! cognition runtime: layer transitions, escalations, resource arbitration,
//! priority shifts, overloads, scheduler interventions.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::cognition_layers::CognitionLayer;

pub static OBS_TOTAL:         AtomicU64 = AtomicU64::new(0);
pub static OBS_ESCALATIONS:   AtomicU64 = AtomicU64::new(0);
pub static OBS_OVERLOADS:     AtomicU64 = AtomicU64::new(0);
pub static OBS_PRIORITY_SHIFTS: AtomicU64 = AtomicU64::new(0);

const MAX_LOG: usize = 500;

// ── Hierarchy observation ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HierarchyObs {
    Escalation      { from: CognitionLayer, to: CognitionLayer, event: String },
    DeEscalation    { from: CognitionLayer, to: CognitionLayer },
    LayerOverload   { layer: CognitionLayer, queue_depth: usize },
    PriorityShift   { old: String, new: String, reason: String },
    ResourceEvent   { resource: String, pressure: f32, action: String },
    SchedulerIntervention { subsystem: String, action: String },
    StrategicPlan   { plan_id: String, goals: usize, horizon_days: u32 },
    HierarchyTick   { tick: u64, layers_active: usize, events_processed: usize },
}

impl HierarchyObs {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Escalation          { .. } => "escalation",
            Self::DeEscalation        { .. } => "de_escalation",
            Self::LayerOverload        { .. } => "layer_overload",
            Self::PriorityShift       { .. } => "priority_shift",
            Self::ResourceEvent       { .. } => "resource_event",
            Self::SchedulerIntervention{..} => "scheduler_intervention",
            Self::StrategicPlan       { .. } => "strategic_plan",
            Self::HierarchyTick       { .. } => "hierarchy_tick",
        }
    }
}

// ── Log record ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObsRecord {
    pub id:       u64,
    pub obs:      HierarchyObs,
    pub severity: ObsSev,
    pub ts_ms:    u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ObsSev { Info, Warn, Critical }

impl ObsSev {
    pub fn label(&self) -> &'static str {
        match self { Self::Info => "INFO", Self::Warn => "WARN", Self::Critical => "CRIT" }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ObsState { log: Vec<ObsRecord>, seq: u64 }

static STATE: Lazy<Mutex<ObsState>> = Lazy::new(|| Mutex::new(ObsState {
    log: Vec::with_capacity(128), seq: 0,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn log(obs: HierarchyObs) {
    OBS_TOTAL.fetch_add(1, Ordering::Relaxed);
    let sev = severity_of(&obs);
    match &obs {
        HierarchyObs::Escalation { .. }   => { OBS_ESCALATIONS.fetch_add(1, Ordering::Relaxed); }
        HierarchyObs::LayerOverload { .. } => { OBS_OVERLOADS.fetch_add(1, Ordering::Relaxed); }
        HierarchyObs::PriorityShift { .. } => { OBS_PRIORITY_SHIFTS.fetch_add(1, Ordering::Relaxed); }
        _ => {}
    }
    if let Ok(mut s) = STATE.lock() {
        s.seq += 1;
        let id = s.seq;
        if s.log.len() >= MAX_LOG { s.log.remove(0); }
        s.log.push(ObsRecord { id, obs, severity: sev, ts_ms: ts_now() });
    }
}

fn severity_of(obs: &HierarchyObs) -> ObsSev {
    match obs {
        HierarchyObs::LayerOverload { queue_depth, .. } if *queue_depth > 20 => ObsSev::Critical,
        HierarchyObs::LayerOverload { .. }     => ObsSev::Warn,
        HierarchyObs::Escalation   { .. }      => ObsSev::Warn,
        HierarchyObs::SchedulerIntervention{..}=> ObsSev::Warn,
        _                                       => ObsSev::Info,
    }
}

pub fn recent(n: usize) -> Vec<ObsRecord> {
    STATE.lock().map(|s| s.log.iter().rev().take(n).cloned().collect()).unwrap_or_default()
}

pub fn snapshot() -> Vec<ObsRecord> {
    STATE.lock().map(|s| s.log.clone()).unwrap_or_default()
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
    fn log_and_retrieve() {
        log(HierarchyObs::HierarchyTick { tick: 1, layers_active: 5, events_processed: 10 });
        let recs = recent(5);
        assert!(recs.iter().any(|r| r.obs.label() == "hierarchy_tick"));
    }

    #[test]
    fn escalation_increments_counter() {
        let before = OBS_ESCALATIONS.load(Ordering::Relaxed);
        log(HierarchyObs::Escalation {
            from: CognitionLayer::Reactive,
            to:   CognitionLayer::Tactical,
            event: "test".into(),
        });
        assert!(OBS_ESCALATIONS.load(Ordering::Relaxed) > before);
    }
}
