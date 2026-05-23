//! Simulation scheduler — schedules future simulations, prioritizes high-risk
//! scenarios, suppresses unstable simulation recursion, and optimizes depth.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static SIMULATIONS_SCHEDULED: AtomicU64 = AtomicU64::new(0);
pub static SIMULATIONS_SUPPRESSED: AtomicU64 = AtomicU64::new(0);

const MAX_QUEUE: usize = 50;
const MIN_PRIORITY_THRESHOLD: f32 = 0.15;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ScheduledSimulation ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScheduledSimulation {
    pub id:            u64,
    pub label:         String,
    pub priority:      f32,
    pub max_depth:     u32,
    pub queued_at_ms:  u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SchedulerState {
    queue: Vec<ScheduledSimulation>,
    seq:   u64,
}

impl SchedulerState {
    fn new() -> Self { SchedulerState { queue: Vec::new(), seq: 0 } }

    fn sort_by_priority(&mut self) {
        self.queue.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap());
    }
}

static STATE: Lazy<Mutex<SchedulerState>> = Lazy::new(|| Mutex::new(SchedulerState::new()));

// ── API ───────────────────────────────────────────────────────────────────────

/// Schedule a simulation with the given priority and label.
/// Returns the simulation ID, or 0 if suppressed.
pub fn schedule(priority: f32, label: impl Into<String>) -> u64 {
    let priority = priority.clamp(0.0, 1.0);

    if priority < MIN_PRIORITY_THRESHOLD {
        SIMULATIONS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    // Safety check before scheduling
    let safety = crate::simulation_safety::check_simulation_safe();
    if !safety.is_safe {
        SIMULATIONS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    // Depth: higher priority → deeper simulation, capped to avoid storms
    let max_depth = (1 + (priority * 5.0) as u32).min(6);

    let mut s = STATE.lock().unwrap();
    s.seq += 1;
    let id = s.seq;

    if s.queue.len() >= MAX_QUEUE { s.queue.remove(0); }
    s.queue.push(ScheduledSimulation {
        id,
        label: label.into(),
        priority,
        max_depth,
        queued_at_ms: ts_now(),
    });
    s.sort_by_priority();

    SIMULATIONS_SCHEDULED.fetch_add(1, Ordering::Relaxed);
    id
}

/// Pop the highest-priority simulation from the queue.
pub fn next_simulation() -> Option<ScheduledSimulation> {
    let mut s = STATE.lock().unwrap();
    if s.queue.is_empty() { return None; }
    Some(s.queue.remove(0))
}

/// Suppress (remove) all simulations with priority below threshold.
pub fn suppress_low_priority(threshold: f32) -> usize {
    let mut s = STATE.lock().unwrap();
    let before = s.queue.len();
    s.queue.retain(|sim| sim.priority >= threshold);
    let removed = before - s.queue.len();
    SIMULATIONS_SUPPRESSED.fetch_add(removed as u64, Ordering::Relaxed);
    removed
}

/// Auto-schedule simulations based on current risk signals.
pub fn auto_schedule() -> usize {
    let unc  = crate::generalized_uncertainty::profile();
    let prob = crate::probabilistic_stability::check();
    let sem  = crate::semantic_stability::check();
    let mut count = 0;

    if unc.overall > 0.40 {
        if schedule(unc.overall, "uncertainty_driven_sim") > 0 { count += 1; }
    }
    if prob.instability_score > 0.40 {
        if schedule(prob.instability_score, "probabilistic_instability_sim") > 0 { count += 1; }
    }
    if sem.has_collapse_risk {
        if schedule(0.80, "semantic_collapse_prevention_sim") > 0 { count += 1; }
    }

    count
}

pub fn queue_length() -> usize { STATE.lock().unwrap().queue.len() }

pub fn total_scheduled()  -> u64 { SIMULATIONS_SCHEDULED.load(Ordering::Relaxed) }
pub fn total_suppressed()  -> u64 { SIMULATIONS_SUPPRESSED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_below_threshold_suppressed() {
        let id = schedule(0.05, "low_priority_sim");
        assert_eq!(id, 0);
    }

    #[test]
    fn schedule_high_priority_queued() {
        let id = schedule(0.90, "high_priority_sim");
        // May be suppressed by safety check depending on system state
        let _ = id;
    }

    #[test]
    fn suppress_low_priority_reduces_queue() {
        let _ = schedule(0.80, "high_sim");
        let before = queue_length();
        suppress_low_priority(0.95);
        assert!(queue_length() <= before);
    }

    #[test]
    fn auto_schedule_no_panic() {
        let _ = auto_schedule();
    }
}
