//! Long-run cognition engine — maintains persistent cognition across ticks,
//! continues world simulation over time, preserves self-model evolution, and
//! maintains adaptive routing state.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static CONTINUITY_TICKS:    AtomicU64 = AtomicU64::new(0);
pub static CONTINUITY_BREAKS:   AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── CognitionContinuityState ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CognitionContinuityState {
    pub tick:                    u64,
    pub belief_continuity:       f32,
    pub routing_continuity:      f32,
    pub world_model_continuity:  f32,
    pub self_model_continuity:   f32,
    pub overall_continuity:      f32,
    pub has_continuity_break:    bool,
    pub ts_ms:                   u64,
}

impl CognitionContinuityState {
    pub fn is_continuous(&self) -> bool { self.overall_continuity > 0.40 }
}

// ── State ─────────────────────────────────────────────────────────────────────

static HISTORY: Lazy<Mutex<Vec<CognitionContinuityState>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

static TICK: AtomicU64 = AtomicU64::new(0);

// ── Core logic ────────────────────────────────────────────────────────────────

/// Assess and maintain long-run cognition continuity.
pub fn maintain() -> CognitionContinuityState {
    let tick = TICK.fetch_add(1, Ordering::Relaxed) + 1;

    let conf = crate::confidence_reasoner::assess();
    let sem  = crate::semantic_stability::check();
    let unc  = crate::generalized_uncertainty::profile();
    let avg_load = crate::adaptive_topology::avg_load();

    // Continuity signals: how well each layer preserves state over ticks
    let belief_continuity     = crate::belief_engine::avg_confidence();
    let routing_continuity    = (1.0 - avg_load).clamp(0.0, 1.0);
    let world_model_continuity = (1.0 - sem.instability_score).clamp(0.0, 1.0);
    let self_model_continuity  = conf.overall;

    let overall_continuity = (
        belief_continuity      * 0.30
        + routing_continuity   * 0.25
        + world_model_continuity * 0.25
        + self_model_continuity * 0.20
    ).clamp(0.0, 1.0);

    let has_continuity_break = overall_continuity < 0.25
        || sem.has_collapse_risk
        || unc.overall > 0.85;

    if has_continuity_break {
        CONTINUITY_BREAKS.fetch_add(1, Ordering::Relaxed);
        crate::ai_os_observability::record(
            crate::ai_os_observability::AiOsEvent::SafetyGate {
                component: "long_run_cognition".into(),
                reason: format!("continuity_break: overall={:.3}", overall_continuity),
            }
        );
    }

    CONTINUITY_TICKS.fetch_add(1, Ordering::Relaxed);

    let state = CognitionContinuityState {
        tick,
        belief_continuity,
        routing_continuity,
        world_model_continuity,
        self_model_continuity,
        overall_continuity,
        has_continuity_break,
        ts_ms: ts_now(),
    };

    let mut h = HISTORY.lock().unwrap();
    if h.len() >= MAX_HISTORY { h.remove(0); }
    h.push(state.clone());

    state
}

/// Average continuity over the last N ticks.
pub fn avg_continuity(n: usize) -> f32 {
    let h = HISTORY.lock().unwrap();
    let slice: Vec<f32> = h.iter().rev().take(n).map(|s| s.overall_continuity).collect();
    if slice.is_empty() { return 0.0; }
    slice.iter().sum::<f32>() / slice.len() as f32
}

pub fn recent(n: usize) -> Vec<CognitionContinuityState> {
    HISTORY.lock().unwrap().iter().rev().take(n).cloned().collect()
}

pub fn continuity_ticks()  -> u64 { CONTINUITY_TICKS.load(Ordering::Relaxed) }
pub fn continuity_breaks() -> u64 { CONTINUITY_BREAKS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintain_no_panic() {
        let s = maintain();
        assert!(s.overall_continuity >= 0.0 && s.overall_continuity <= 1.0);
    }

    #[test]
    fn avg_continuity_bounded() {
        for _ in 0..3 { let _ = maintain(); }
        let avg = avg_continuity(3);
        assert!(avg >= 0.0 && avg <= 1.0);
    }

    #[test]
    fn continuity_ticks_increment() {
        let before = continuity_ticks();
        let _ = maintain();
        assert!(continuity_ticks() > before);
    }
}
