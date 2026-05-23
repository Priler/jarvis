//! Topology memory — persistent log of routing decisions, topology changes,
//! scheduler adaptations, optimizations, and stability interventions.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static EVENTS_LOGGED: AtomicU64 = AtomicU64::new(0);

const MAX_EVENTS: usize = 500;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── TopologyEvent ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TopologyEvent {
    RoutingDecision      { from: String, to: String, reason: String },
    TopologyChange       { component: String, old_state: String, new_state: String },
    SchedulerAdaptation  { target: String, action: String, delta: f32 },
    OptimizationApplied  { target: String, before: f32, after: f32 },
    StabilityIntervention{ component: String, reason: String },
    PathSuppressed       { path: String, load: f32 },
    PathRestored         { path: String },
}

impl TopologyEvent {
    pub fn tag(&self) -> &str {
        match self {
            Self::RoutingDecision { .. }       => "[ROUTER]",
            Self::TopologyChange { .. }        => "[TOPOLOGY]",
            Self::SchedulerAdaptation { .. }   => "[ADAPTIVE]",
            Self::OptimizationApplied { .. }   => "[OPTIMIZATION]",
            Self::StabilityIntervention { .. } => "[EVOLUTION]",
            Self::PathSuppressed { .. }        => "[RESTRUCTURE]",
            Self::PathRestored { .. }          => "[RESTRUCTURE]",
        }
    }

    pub fn is_topology_change(&self) -> bool {
        matches!(self, Self::TopologyChange { .. } | Self::PathSuppressed { .. } | Self::PathRestored { .. })
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static MEMORY: Lazy<Mutex<Vec<(u64, TopologyEvent)>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── API ───────────────────────────────────────────────────────────────────────

pub fn record(event: TopologyEvent) {
    let mut m = MEMORY.lock().unwrap();
    if m.len() >= MAX_EVENTS { m.remove(0); }
    m.push((ts_now(), event));
    EVENTS_LOGGED.fetch_add(1, Ordering::Relaxed);
}

pub fn recent(n: usize) -> Vec<(u64, TopologyEvent)> {
    MEMORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

/// Count topology-change events in the last `window` events.
pub fn recent_topology_changes(window: usize) -> usize {
    MEMORY.lock().unwrap().iter().rev().take(window)
        .filter(|(_, e)| e.is_topology_change())
        .count()
}

pub fn event_count() -> usize { MEMORY.lock().unwrap().len() }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        record(TopologyEvent::RoutingDecision {
            from:   "symbolic".into(),
            to:     "probabilistic".into(),
            reason: "high_uncertainty".into(),
        });
        assert!(event_count() >= 1);
    }

    #[test]
    fn recent_topology_changes_counts_correctly() {
        record(TopologyEvent::PathSuppressed { path: "test_path".into(), load: 0.9 });
        let count = recent_topology_changes(50);
        assert!(count >= 1);
    }
}
