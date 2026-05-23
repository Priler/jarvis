//! Distributed runtime — distributes cognition workloads across local worker
//! runtimes, maintains a distributed cognition mesh, and rebalances loads.
//! Fully local: no cloud nodes, no external workers, no telemetry.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static WORKLOADS_DISPATCHED:  AtomicU64 = AtomicU64::new(0);
pub static REBALANCE_CYCLES:      AtomicU64 = AtomicU64::new(0);

const MAX_LOCAL_WORKERS: usize = 4;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── LocalWorker ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LocalWorker {
    pub id:          usize,
    pub label:       String,
    pub load:        f32,
    pub is_active:   bool,
    pub last_tick_ms: u64,
}

impl LocalWorker {
    pub fn is_overloaded(&self) -> bool { self.load > 0.75 }
}

// ── Workload ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DistributedWorkload {
    pub id:       u64,
    pub label:    String,
    pub worker:   usize,
    pub load:     f32,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct DistributedState {
    workers:  Vec<LocalWorker>,
    seq:      u64,
}

impl DistributedState {
    fn new() -> Self {
        let workers = (0..MAX_LOCAL_WORKERS)
            .map(|i| LocalWorker {
                id: i,
                label: format!("local_worker_{i}"),
                load: 0.20,
                is_active: true,
                last_tick_ms: 0,
            })
            .collect();
        DistributedState { workers, seq: 0 }
    }

    fn least_loaded_worker(&self) -> Option<usize> {
        self.workers.iter()
            .filter(|w| w.is_active && !w.is_overloaded())
            .min_by(|a, b| a.load.partial_cmp(&b.load).unwrap())
            .map(|w| w.id)
    }
}

static STATE: Lazy<Mutex<DistributedState>> =
    Lazy::new(|| Mutex::new(DistributedState::new()));

// ── API ───────────────────────────────────────────────────────────────────────

/// Dispatch a workload to the least-loaded local worker.
/// Returns the workload ID or 0 if no worker is available.
pub fn dispatch(label: impl Into<String>, load: f32) -> u64 {
    let safe = crate::distributed_safety::check_distributed_safe();
    if !safe.is_safe { return 0; }

    let mut s = STATE.lock().unwrap();
    let worker_id = match s.least_loaded_worker() {
        Some(id) => id,
        None     => return 0,
    };

    s.seq += 1;
    let id = s.seq;
    let load = load.clamp(0.0, 1.0);

    if let Some(w) = s.workers.get_mut(worker_id) {
        w.load = (w.load * 0.80 + load * 0.20).clamp(0.0, 1.0);
        w.last_tick_ms = ts_now();
    }

    WORKLOADS_DISPATCHED.fetch_add(1, Ordering::Relaxed);

    crate::ai_os_observability::record(
        crate::ai_os_observability::AiOsEvent::SchedulerDecision {
            job:      label.into(),
            priority: load,
        }
    );

    id
}

/// Rebalance worker loads: decay all loads toward the global average.
pub fn rebalance() {
    let avg = crate::adaptive_topology::avg_load();
    let mut s = STATE.lock().unwrap();
    for w in s.workers.iter_mut() {
        if w.is_active {
            w.load = (w.load * 0.85 + avg * 0.15).clamp(0.0, 1.0);
            w.last_tick_ms = ts_now();
        }
    }
    REBALANCE_CYCLES.fetch_add(1, Ordering::Relaxed);
    crate::ai_os_observability::record(
        crate::ai_os_observability::AiOsEvent::DistributedRebalance {
            workers:  MAX_LOCAL_WORKERS,
            avg_load: avg,
        }
    );
}

pub fn workers()          -> Vec<LocalWorker> { STATE.lock().unwrap().workers.clone() }
pub fn active_workers()   -> usize { STATE.lock().unwrap().workers.iter().filter(|w| w.is_active).count() }
pub fn avg_worker_load()  -> f32 {
    let s = STATE.lock().unwrap();
    let active: Vec<f32> = s.workers.iter().filter(|w| w.is_active).map(|w| w.load).collect();
    if active.is_empty() { return 0.0; }
    active.iter().sum::<f32>() / active.len() as f32
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workers_initialised() {
        assert_eq!(workers().len(), MAX_LOCAL_WORKERS);
    }

    #[test]
    fn dispatch_no_panic() {
        let _ = dispatch("test_workload", 0.30);
    }

    #[test]
    fn rebalance_keeps_loads_bounded() {
        rebalance();
        let avg = avg_worker_load();
        assert!(avg >= 0.0 && avg <= 1.0);
    }
}
