//! Meta-cognition scheduler — governs cadence for each meta-cognitive subsystem.
//! Prevents overloading the cognition runtime and suppresses recursive reasoning
//! storms by enforcing minimum inter-run intervals per subsystem.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static SCHEDULER_TICKS:    AtomicU64 = AtomicU64::new(0);
pub static SUBSYSTEMS_SKIPPED: AtomicU64 = AtomicU64::new(0);
pub static SUBSYSTEMS_RUN:     AtomicU64 = AtomicU64::new(0);

// ── Cadence table (milliseconds between allowed runs) ─────────────────────────

const CADENCE_META_CYCLE:      u64 = 3_000;   // full meta-cognition cycle
const CADENCE_UNCERTAINTY:     u64 = 2_000;   // uncertainty recalibration
const CADENCE_CAUSAL:          u64 = 5_000;   // causal chain analysis
const CADENCE_REFLECTION:      u64 = 8_000;   // meta-reflection
const CADENCE_SIMULATION:      u64 = 4_000;   // strategy simulation
const CADENCE_ARBITRATION:     u64 = 3_000;   // strategic arbitration
const CADENCE_COUNTERFACTUAL:  u64 = 10_000;  // counterfactual evaluation
const CADENCE_WATCHDOG:        u64 = 2_000;   // cognitive watchdog
const CADENCE_MEMORY_FUSION:   u64 = 15_000;  // memory fusion consolidation

// ── Subsystem identifiers ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Subsystem {
    MetaCycle,
    Uncertainty,
    CausalAnalysis,
    Reflection,
    Simulation,
    Arbitration,
    Counterfactual,
    Watchdog,
    MemoryFusion,
}

impl Subsystem {
    fn cadence_ms(self) -> u64 {
        match self {
            Self::MetaCycle      => CADENCE_META_CYCLE,
            Self::Uncertainty    => CADENCE_UNCERTAINTY,
            Self::CausalAnalysis => CADENCE_CAUSAL,
            Self::Reflection     => CADENCE_REFLECTION,
            Self::Simulation     => CADENCE_SIMULATION,
            Self::Arbitration    => CADENCE_ARBITRATION,
            Self::Counterfactual => CADENCE_COUNTERFACTUAL,
            Self::Watchdog       => CADENCE_WATCHDOG,
            Self::MemoryFusion   => CADENCE_MEMORY_FUSION,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MetaCycle      => "meta_cycle",
            Self::Uncertainty    => "uncertainty",
            Self::CausalAnalysis => "causal_analysis",
            Self::Reflection     => "reflection",
            Self::Simulation     => "simulation",
            Self::Arbitration    => "arbitration",
            Self::Counterfactual => "counterfactual",
            Self::Watchdog       => "watchdog",
            Self::MemoryFusion   => "memory_fusion",
        }
    }
}

// ── Schedule state ────────────────────────────────────────────────────────────

struct SchedulerState {
    last_run: HashMap<Subsystem, u64>,
}

static STATE: Lazy<Mutex<SchedulerState>> = Lazy::new(|| Mutex::new(SchedulerState {
    last_run: HashMap::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns true if this subsystem is due to run.
/// Records the run timestamp if due, so repeated calls within the same ms window skip.
pub fn is_due(sub: Subsystem) -> bool {
    SCHEDULER_TICKS.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    if let Ok(mut s) = STATE.lock() {
        let last = s.last_run.get(&sub).copied().unwrap_or(0);
        if now.saturating_sub(last) >= sub.cadence_ms() {
            s.last_run.insert(sub, now);
            SUBSYSTEMS_RUN.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            SUBSYSTEMS_SKIPPED.fetch_add(1, Ordering::Relaxed);
            false
        }
    } else {
        false
    }
}

/// Force-reset a subsystem's last-run timestamp (used by watchdog to suppress).
pub fn suppress(sub: Subsystem) {
    if let Ok(mut s) = STATE.lock() {
        // Set last_run to "far future" so it won't run until the freeze clears
        s.last_run.insert(sub, ts_now() + 60_000);
    }
}

/// Allow a subsystem to run immediately on next tick.
pub fn allow_now(sub: Subsystem) {
    if let Ok(mut s) = STATE.lock() {
        s.last_run.insert(sub, 0);
    }
}

/// Returns milliseconds until the subsystem is next due (0 = due now).
pub fn time_until_due(sub: Subsystem) -> u64 {
    let now = ts_now();
    STATE.lock().map(|s| {
        let last = s.last_run.get(&sub).copied().unwrap_or(0);
        let elapsed = now.saturating_sub(last);
        sub.cadence_ms().saturating_sub(elapsed)
    }).unwrap_or(0)
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
    fn watchdog_always_due_after_reset() {
        allow_now(Subsystem::Watchdog);
        assert!(is_due(Subsystem::Watchdog));
    }

    #[test]
    fn second_call_within_cadence_skips() {
        allow_now(Subsystem::MemoryFusion);
        assert!(is_due(Subsystem::MemoryFusion));
        // immediately after: cadence not elapsed → skip
        assert!(!is_due(Subsystem::MemoryFusion));
    }

    #[test]
    fn suppress_prevents_run() {
        allow_now(Subsystem::Counterfactual);
        suppress(Subsystem::Counterfactual);
        assert!(!is_due(Subsystem::Counterfactual));
    }
}
