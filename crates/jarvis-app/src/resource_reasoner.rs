//! Resource reasoning engine — reasons about CPU, memory, latency, cognition
//! load, simulation load, and scheduler pressure.
//! Publishes ResourcePressure events when thresholds are exceeded.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static RESOURCE_SAMPLES:    AtomicU64 = AtomicU64::new(0);
pub static PRESSURE_EVENTS:     AtomicU64 = AtomicU64::new(0);
pub static THROTTLE_DECISIONS:  AtomicU64 = AtomicU64::new(0);

const HIGH_PRESSURE_THRESH: f32 = 0.75;
const CRITICAL_THRESH:      f32 = 0.90;
const MAX_HISTORY:          usize = 100;

// ── Resource reading ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceReading {
    pub resource:  ResourceKind,
    pub pressure:  f32,    // 0–1
    pub trend:     Trend,
    pub ts_ms:     u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ResourceKind {
    CognitionLoad,
    SimulationLoad,
    SchedulerPressure,
    MetaOverhead,
    HierarchyDepth,
}

impl ResourceKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CognitionLoad     => "cognition_load",
            Self::SimulationLoad    => "simulation_load",
            Self::SchedulerPressure => "scheduler_pressure",
            Self::MetaOverhead      => "meta_overhead",
            Self::HierarchyDepth    => "hierarchy_depth",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Trend { Rising, Stable, Falling }

// ── Resource snapshot ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceSnapshot {
    pub readings:       Vec<ResourceReading>,
    pub overall:        f32,
    pub critical_count: usize,
    pub ts_ms:          u64,
}

impl ResourceSnapshot {
    pub fn is_overloaded(&self) -> bool { self.overall >= HIGH_PRESSURE_THRESH }
    pub fn is_critical(&self)   -> bool { self.critical_count > 0 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ResourceState {
    history: Vec<ResourceSnapshot>,
    prev:    Option<ResourceSnapshot>,
}

static STATE: Lazy<Mutex<ResourceState>> = Lazy::new(|| Mutex::new(ResourceState {
    history: Vec::new(),
    prev:    None,
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn sample() -> ResourceSnapshot {
    RESOURCE_SAMPLES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    // Derive pressure from runtime counters (heuristic — no syscalls)
    let meta_cycles    = crate::meta_cognition_runtime::META_CYCLES.load(Ordering::Relaxed);
    let sims_run       = crate::strategy_simulator::SIMULATIONS_RUN.load(Ordering::Relaxed);
    let sched_ticks    = crate::meta_scheduler::SCHEDULER_TICKS.load(Ordering::Relaxed);
    let hier_events    = crate::cognition_layers::EVENTS_ROUTED.load(Ordering::Relaxed);
    let watchdog_checks = crate::cognitive_watchdog::WATCHDOG_CHECKS.load(Ordering::Relaxed);

    // Normalise to [0,1] using expected rates (heuristic)
    let cognition_p = (meta_cycles as f32 / 1000.0).min(1.0);
    let sim_p       = (sims_run    as f32 / 500.0).min(1.0);
    let sched_p     = (sched_ticks as f32 / 5000.0).min(1.0);
    let meta_p      = (watchdog_checks as f32 / 2000.0).min(1.0);
    let hier_p      = (hier_events  as f32 / 2000.0).min(1.0);

    let readings = vec![
        mk_reading(ResourceKind::CognitionLoad,     cognition_p, now),
        mk_reading(ResourceKind::SimulationLoad,    sim_p,       now),
        mk_reading(ResourceKind::SchedulerPressure, sched_p,     now),
        mk_reading(ResourceKind::MetaOverhead,      meta_p,      now),
        mk_reading(ResourceKind::HierarchyDepth,    hier_p,      now),
    ];

    let overall = readings.iter().map(|r| r.pressure).sum::<f32>() / readings.len() as f32;
    let critical_count = readings.iter().filter(|r| r.pressure >= CRITICAL_THRESH).count();

    if overall >= HIGH_PRESSURE_THRESH {
        PRESSURE_EVENTS.fetch_add(1, Ordering::Relaxed);
    }

    let snap = ResourceSnapshot { readings, overall, critical_count, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(snap.clone());
        s.prev = Some(snap.clone());
    }

    snap
}

/// Recommend throttle action based on current resource state.
pub fn throttle_recommendation(snap: &ResourceSnapshot) -> Option<&'static str> {
    if snap.is_critical() {
        THROTTLE_DECISIONS.fetch_add(1, Ordering::Relaxed);
        Some("suppress_simulation_and_reflection")
    } else if snap.is_overloaded() {
        THROTTLE_DECISIONS.fetch_add(1, Ordering::Relaxed);
        Some("increase_scheduler_cadences")
    } else {
        None
    }
}

pub fn latest() -> Option<ResourceSnapshot> {
    STATE.lock().ok().and_then(|s| s.prev.clone())
}

fn mk_reading(resource: ResourceKind, pressure: f32, ts_ms: u64) -> ResourceReading {
    ResourceReading { resource, pressure, trend: Trend::Stable, ts_ms }
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
    fn sample_returns_snapshot() {
        let snap = sample();
        assert_eq!(snap.readings.len(), 5);
        assert!(snap.overall >= 0.0 && snap.overall <= 1.0);
        assert!(RESOURCE_SAMPLES.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn latest_available_after_sample() {
        sample();
        assert!(latest().is_some());
    }

    #[test]
    fn throttle_none_when_low() {
        let snap = ResourceSnapshot {
            readings: vec![], overall: 0.3, critical_count: 0, ts_ms: 0 };
        assert!(throttle_recommendation(&snap).is_none());
    }
}
