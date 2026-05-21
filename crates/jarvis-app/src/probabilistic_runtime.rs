//! Probabilistic generalized intelligence runtime — orchestrates all Phase 22
//! subsystems in a 10-step tick loop at 2500 ms intervals.

use std::sync::atomic::{AtomicBool, Ordering};
use once_cell::sync::Lazy;

static RUNNING: AtomicBool = AtomicBool::new(false);
static STOP:    AtomicBool = AtomicBool::new(false);

const TICK_INTERVAL_MS: u64 = 2500;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── TickResult ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProbabilisticTickResult {
    pub belief_count:       usize,
    pub avg_confidence:     f32,
    pub uncertainty_score:  f32,
    pub is_stable:          bool,
    pub predictions_made:   usize,
    pub ts_ms:              u64,
}

// ── Tick ──────────────────────────────────────────────────────────────────────

pub fn run_tick() -> ProbabilisticTickResult {
    // 1. Sample generalized uncertainty profile
    let unc_profile = crate::generalized_uncertainty::profile();

    // 2. Decay all beliefs
    crate::belief_engine::decay_all();

    // 3. Seed and propagate uncertainty graph from beliefs + chains
    crate::belief_propagation::seed_from_beliefs();
    crate::belief_propagation::seed_from_chains();
    let _prop = crate::belief_propagation::propagate(3);

    // 4. Assess confidence
    let conf_rep = crate::confidence_reasoner::assess();

    // 5. Update semantic self-model
    let self_snap = crate::semantic_self_model::sample();

    // 6. Update probabilistic world model
    let _world = crate::probabilistic_world_model::snapshot();

    // 7. Run probabilistic inference (rank hypotheses)
    let _hypotheses = crate::probabilistic_inference::rank_hypotheses();

    // 8. Detect and resolve probabilistic contradictions
    let _ = crate::probabilistic_contradictions::detect_conflicts();
    let _ = crate::probabilistic_contradictions::resolve_by_confidence();

    // 9. Check probabilistic stability
    let stability = crate::probabilistic_stability::check();

    // 10. Run predictions + safety checks + self-model evolution
    let predictions = crate::predictive_intelligence::predict_failures();
    let _evolution  = crate::self_model_evolution::evolve();
    crate::self_model_evolution::recalibrate_confidence();

    // Safety checks
    crate::probabilistic_safety::check_uncertainty_explosion();
    crate::probabilistic_safety::check_belief_collapse();

    // Log uncertainty drift
    crate::probabilistic_observability::log(
        crate::probabilistic_observability::ProbabilisticEvent::UncertaintyDrift {
            delta:     unc_profile.overall,
            direction: if unc_profile.overall > 0.50 { "rising".into() } else { "stable".into() },
        }
    );

    ProbabilisticTickResult {
        belief_count:      crate::belief_engine::belief_count(),
        avg_confidence:    conf_rep.overall,
        uncertainty_score: unc_profile.overall,
        is_stable:         stability.is_stable && self_snap.is_healthy,
        predictions_made:  predictions.len(),
        ts_ms:             ts_now(),
    }
}

// ── Thread lifecycle ──────────────────────────────────────────────────────────

pub fn start() {
    if RUNNING.load(Ordering::SeqCst) { return; }
    STOP.store(false, Ordering::SeqCst);
    std::thread::Builder::new()
        .name("jarvis-probabilistic-runtime".to_string())
        .spawn(|| {
            RUNNING.store(true, Ordering::SeqCst);
            while !STOP.load(Ordering::SeqCst) {
                let _ = std::panic::catch_unwind(|| run_tick());
                std::thread::sleep(std::time::Duration::from_millis(TICK_INTERVAL_MS));
            }
            RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
}

pub fn is_running() -> bool { RUNNING.load(Ordering::SeqCst) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_produces_sane_result() {
        let r = run_tick();
        assert!(r.avg_confidence   >= 0.0 && r.avg_confidence   <= 1.0);
        assert!(r.uncertainty_score >= 0.0 && r.uncertainty_score <= 1.0);
    }

    #[test]
    fn start_stop_no_panic() {
        start();
        std::thread::sleep(std::time::Duration::from_millis(50));
        stop();
    }
}
