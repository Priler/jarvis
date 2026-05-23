//! Adaptive scheduler — throttles overloaded cognition paths, prioritises stable
//! ones, and prevents unstable recursion from consuming all scheduling budget.

use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

pub static THROTTLE_DECISIONS: AtomicU64 = AtomicU64::new(0);
pub static RESTORES_ISSUED:    AtomicU64 = AtomicU64::new(0);

const MILD_THROTTLE_THRESHOLD:  f32 = 0.60;
const HEAVY_THROTTLE_THRESHOLD: f32 = 0.78;
// After this many ticks suppressed, a path is eligible for auto-restore
const AUTO_RESTORE_TICKS: u32 = 4;

use crate::adaptive_topology::CognitionPath;

// ── ThrottleLevel ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThrottleLevel {
    None,
    Mild   { factor: f32 },  // run at reduced frequency (factor 0–1)
    Heavy  { factor: f32 },  // run very infrequently
}

impl ThrottleLevel {
    pub fn should_run(&self, tick: u64) -> bool {
        match self {
            Self::None              => true,
            Self::Mild  { factor } => (tick as f32 * factor) as u64 % 3 == 0,
            Self::Heavy { factor } => (tick as f32 * factor) as u64 % 8 == 0,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::None       => "none",
            Self::Mild  {..} => "mild",
            Self::Heavy {..} => "heavy",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SchedulerState {
    throttles:      HashMap<CognitionPath, ThrottleLevel>,
    suppressed_for: HashMap<CognitionPath, u32>,  // ticks suppressed
    tick:           u64,
}

impl SchedulerState {
    fn new() -> Self {
        let mut throttles = HashMap::new();
        for p in CognitionPath::all() {
            throttles.insert(*p, ThrottleLevel::None);
        }
        SchedulerState { throttles, suppressed_for: HashMap::new(), tick: 0 }
    }
}

static STATE: Lazy<Mutex<SchedulerState>> = Lazy::new(|| Mutex::new(SchedulerState::new()));

// ── API ───────────────────────────────────────────────────────────────────────

/// Adapt throttle levels based on current adaptive_topology loads.
pub fn adapt() {
    let loads = crate::adaptive_topology::all_loads();
    let mut s = STATE.lock().unwrap();
    s.tick += 1;
    let tick = s.tick;

    for pl in &loads {
        if pl.suppressed {
            // Track how long this path has been suppressed
            let count = s.suppressed_for.entry(pl.path).or_insert(0);
            *count += 1;
        } else {
            // Auto-restore from heavy throttle if load dropped
            if pl.load_score < MILD_THROTTLE_THRESHOLD {
                if matches!(s.throttles.get(&pl.path), Some(ThrottleLevel::Heavy { .. })) {
                    let ticks = *s.suppressed_for.get(&pl.path).unwrap_or(&0);
                    if ticks >= AUTO_RESTORE_TICKS {
                        s.throttles.insert(pl.path, ThrottleLevel::None);
                        s.suppressed_for.remove(&pl.path);
                        RESTORES_ISSUED.fetch_add(1, Ordering::Relaxed);
                        crate::topology_memory::record(crate::topology_memory::TopologyEvent::SchedulerAdaptation {
                            target: pl.path.name().into(),
                            action: "auto_restore".into(),
                            delta:  -(pl.load_score),
                        });
                    }
                } else {
                    s.throttles.insert(pl.path, ThrottleLevel::None);
                    s.suppressed_for.remove(&pl.path);
                }
            } else if pl.load_score > HEAVY_THROTTLE_THRESHOLD {
                s.throttles.insert(pl.path, ThrottleLevel::Heavy { factor: 0.20 });
                let count = s.suppressed_for.entry(pl.path).or_insert(0);
                *count += 1;
                THROTTLE_DECISIONS.fetch_add(1, Ordering::Relaxed);
                crate::topology_memory::record(crate::topology_memory::TopologyEvent::SchedulerAdaptation {
                    target: pl.path.name().into(),
                    action: "heavy_throttle".into(),
                    delta:  pl.load_score,
                });
            } else if pl.load_score > MILD_THROTTLE_THRESHOLD {
                s.throttles.insert(pl.path, ThrottleLevel::Mild { factor: 0.50 });
                THROTTLE_DECISIONS.fetch_add(1, Ordering::Relaxed);
                crate::topology_memory::record(crate::topology_memory::TopologyEvent::SchedulerAdaptation {
                    target: pl.path.name().into(),
                    action: "mild_throttle".into(),
                    delta:  pl.load_score,
                });
            }
        }
        let _ = tick;
    }
}

pub fn throttle_level(path: CognitionPath) -> ThrottleLevel {
    STATE.lock().unwrap().throttles.get(&path).copied().unwrap_or(ThrottleLevel::None)
}

pub fn should_run(path: CognitionPath, tick: u64) -> bool {
    let level = STATE.lock().unwrap().throttles.get(&path).copied().unwrap_or(ThrottleLevel::None);
    level.should_run(tick)
}

pub fn current_tick() -> u64 { STATE.lock().unwrap().tick }

pub fn throttled_paths() -> Vec<CognitionPath> {
    STATE.lock().unwrap().throttles.iter()
        .filter(|(_, t)| !matches!(t, ThrottleLevel::None))
        .map(|(p, _)| *p)
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapt_no_panic() {
        adapt();
    }

    #[test]
    fn throttle_none_always_runs() {
        let level = ThrottleLevel::None;
        assert!(level.should_run(0));
        assert!(level.should_run(100));
    }

    #[test]
    fn throttle_level_symbolic_default_none() {
        let t = throttle_level(CognitionPath::Symbolic);
        let _ = t.label();
    }
}
