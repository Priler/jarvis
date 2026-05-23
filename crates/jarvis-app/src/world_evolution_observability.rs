//! World evolution observability — event log for generalized world simulation,
//! cognitive synthesis, and topology generation activity.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static EVENTS_RECORDED: AtomicU64 = AtomicU64::new(0);

const MAX_LOG: usize = 500;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── WorldSimEvent ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum WorldSimEvent {
    SimulationRun       { scenario_id: u64, outcome: String, instability: f32 },
    WorldModelUpdated   { component: String, delta: f32 },
    CognitionSynthesized{ label: String, confidence: f32 },
    TopologyGenerated   { label: String, stability: f32 },
    SafetyIntervention  { component: String, reason: String },
    SimulationSuppressed{ reason: String },
    FuturePredicted     { horizon_ticks: u32, instability: f32 },
}

impl WorldSimEvent {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::SimulationRun       { .. } => "[SIM_RUN]",
            Self::WorldModelUpdated   { .. } => "[WORLD_MODEL]",
            Self::CognitionSynthesized{ .. } => "[SYNTHESIS]",
            Self::TopologyGenerated   { .. } => "[TOPO_GEN]",
            Self::SafetyIntervention  { .. } => "[SAFETY]",
            Self::SimulationSuppressed{ .. } => "[SUPPRESSED]",
            Self::FuturePredicted     { .. } => "[PREDICTED]",
        }
    }

    pub fn severity(&self) -> u8 {
        match self {
            Self::SafetyIntervention  { .. } => 3,
            Self::SimulationSuppressed{ .. } => 2,
            Self::WorldModelUpdated   { .. } => 1,
            _                                => 0,
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static LOG: Lazy<Mutex<Vec<(u64, WorldSimEvent)>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── API ───────────────────────────────────────────────────────────────────────

pub fn record(event: WorldSimEvent) {
    let ts = ts_now();
    let mut log = LOG.lock().unwrap();
    if log.len() >= MAX_LOG { log.remove(0); }
    log.push((ts, event));
    EVENTS_RECORDED.fetch_add(1, Ordering::Relaxed);
}

pub fn recent(n: usize) -> Vec<(u64, WorldSimEvent)> {
    LOG.lock().unwrap().iter().rev().take(n).cloned().collect()
}

pub fn event_count() -> usize {
    LOG.lock().unwrap().len()
}

pub fn safety_interventions() -> usize {
    LOG.lock().unwrap().iter()
        .filter(|(_, e)| matches!(e, WorldSimEvent::SafetyIntervention { .. }))
        .count()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        record(WorldSimEvent::FuturePredicted { horizon_ticks: 10, instability: 0.3 });
        assert!(event_count() > 0);
        let r = recent(1);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn severity_ordering() {
        let e = WorldSimEvent::SafetyIntervention { component: "x".into(), reason: "y".into() };
        assert_eq!(e.severity(), 3);
    }
}
