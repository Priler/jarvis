//! Distributed memory bus — shares cognition state, synchronises belief
//! structures, propagates routing decisions, and coordinates distributed services.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static SYNCS_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static PROPAGATIONS:    AtomicU64 = AtomicU64::new(0);

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── BusState ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SharedCognitionState {
    pub belief_avg_confidence:  f32,
    pub routing_avg_load:       f32,
    pub uncertainty_overall:    f32,
    pub semantic_instability:   f32,
    pub future_memory_count:    usize,
    pub updated_ms:             u64,
}

impl SharedCognitionState {
    pub fn is_coherent(&self) -> bool {
        self.belief_avg_confidence > 0.25
            && self.uncertainty_overall < 0.85
            && self.semantic_instability < 0.80
    }
}

struct BusStore {
    current: Option<SharedCognitionState>,
}

static BUS: Lazy<Mutex<BusStore>> = Lazy::new(|| Mutex::new(BusStore { current: None }));

// ── API ───────────────────────────────────────────────────────────────────────

/// Sync all live signals into the shared bus state.
pub fn sync() -> SharedCognitionState {
    let state = SharedCognitionState {
        belief_avg_confidence: crate::belief_engine::avg_confidence(),
        routing_avg_load:      crate::adaptive_topology::avg_load(),
        uncertainty_overall:   crate::generalized_uncertainty::profile().overall,
        semantic_instability:  crate::semantic_stability::check().instability_score,
        future_memory_count:   crate::future_memory::count(),
        updated_ms:            ts_now(),
    };

    BUS.lock().unwrap().current = Some(state.clone());
    SYNCS_COMPLETED.fetch_add(1, Ordering::Relaxed);
    state
}

/// Propagate current state to the topology memory and observability layer.
pub fn propagate() {
    let state = {
        BUS.lock().unwrap().current.clone()
    };
    if let Some(s) = state {
        // Propagate routing load to topology memory
        crate::topology_memory::record(
            crate::topology_memory::TopologyEvent::SchedulerAdaptation {
                target: "distributed_memory_bus".into(),
                action: "propagate".into(),
                delta:  s.routing_avg_load,
            }
        );
        PROPAGATIONS.fetch_add(1, Ordering::Relaxed);

        crate::ai_os_observability::record(
            crate::ai_os_observability::AiOsEvent::DistributedRebalance {
                workers: 1,
                avg_load: s.routing_avg_load,
            }
        );
    }
}

pub fn current() -> Option<SharedCognitionState> {
    BUS.lock().unwrap().current.clone()
}

pub fn syncs_completed()  -> u64 { SYNCS_COMPLETED.load(Ordering::Relaxed) }
pub fn propagations()     -> u64 { PROPAGATIONS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_no_panic() {
        let s = sync();
        assert!(s.routing_avg_load >= 0.0 && s.routing_avg_load <= 1.0);
    }

    #[test]
    fn current_after_sync_is_some() {
        sync();
        assert!(current().is_some());
    }

    #[test]
    fn propagate_no_panic() {
        sync();
        propagate();
    }
}
