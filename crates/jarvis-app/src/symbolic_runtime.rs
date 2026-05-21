//! Symbolic runtime — central Phase 21 coordinator.
//! Background thread "jarvis-symbolic-runtime" at 3000ms cadence.
//! 10-step tick: safety → graph_update → inference → contradiction →
//!               synthesis → world_model → constraints → stability →
//!               inference_planning → observability.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use once_cell::sync::Lazy;

pub static TICKS_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static TICK_ERRORS: AtomicU64 = AtomicU64::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);
static STOP:    AtomicBool = AtomicBool::new(false);

const TICK_INTERVAL_MS: u64 = 3000;

// ── Tick result ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolicTick {
    pub tick_id:              u64,
    pub inference_chains:     usize,
    pub contradictions:       usize,
    pub syntheses:            usize,
    pub constraint_violations: usize,
    pub semantic_quality:     f32,
    pub world_state:          String,
    pub is_stable:            bool,
    pub healthy:              bool,
    pub duration_ms:          u64,
}

// ── Tick implementation ───────────────────────────────────────────────────────

pub fn run_tick() -> SymbolicTick {
    let tick_id = TICKS_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    let start_ms = ts_now();

    // Step 1 — Safety: validate existing chains
    let safety_ok = {
        let chains = crate::symbolic_inference::all_chains();
        let rejected = chains.iter()
            .filter(|c| !crate::symbolic_safety::validate_chain(c.depth, c.confidence).is_valid())
            .count();
        rejected == 0 || rejected < chains.len() / 2
    };

    // Step 2 — Semantic graph: populate from causal links + concepts
    {
        let links = crate::causal_reasoner::reliable_links();
        for link in links.iter().take(10) {
            crate::semantic_graph::relate(
                &link.cause, crate::semantic_graph::EntityKind::Concept,
                &link.effect, crate::semantic_graph::EntityKind::Concept,
                crate::semantic_graph::SemanticRelation::Causal, link.strength,
            );
        }
        // Transitive inference pass
        crate::semantic_graph::infer_transitive();
    }

    // Step 3 — Full semantic reasoning cycle
    let sem_result = crate::semantic_reasoner::reason();
    let inference_chains  = sem_result.inference_chains_found;
    let contradictions     = sem_result.contradictions_detected;
    let syntheses          = sem_result.syntheses_created;
    let constraint_violations = sem_result.constraint_violations;
    let semantic_quality   = sem_result.semantic_quality;

    // Step 4 — Semantic transfer: transfer top chain to top concept domain
    {
        let chains = crate::symbolic_inference::reliable_chains();
        let concepts = crate::concept_engine::reliable_concepts();
        if let (Some(chain), Some(target)) = (chains.first(), concepts.get(1)) {
            crate::semantic_transfer::transfer_chain(&chain.root, &target.label);
        }
    }

    // Step 5 — Causal semantic analysis
    crate::abstract_causal_semantics::run_analysis();

    // Step 6 — World model update
    let world_snap = crate::symbolic_world_model::update();
    let world_state = format!("{:?}", world_snap.symbolic_state);

    // Step 7 — Semantic composition
    crate::semantic_composition::auto_compose();

    // Step 8 — Semantic stability check
    let stability = crate::semantic_stability::check();
    let is_stable = stability.is_stable;

    // Step 9 — Inference planning from reliable chains
    crate::inference_planner::plan_all_reliable();

    // Step 10 — Observability tick log
    let healthy = safety_ok && sem_result.healthy && is_stable;
    crate::symbolic_observability::log(
        crate::symbolic_observability::SymbolicEvent::SymbolicTick {
            tick_id, chains: inference_chains, contradictions, syntheses, healthy,
        }
    );

    let duration_ms = ts_now().saturating_sub(start_ms);
    SymbolicTick {
        tick_id, inference_chains, contradictions, syntheses,
        constraint_violations, semantic_quality, world_state,
        is_stable, healthy, duration_ms,
    }
}

// ── Background thread ─────────────────────────────────────────────────────────

pub fn start() {
    if RUNNING.swap(true, Ordering::SeqCst) { return; }
    STOP.store(false, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("jarvis-symbolic-runtime".to_string())
        .spawn(move || {
            while !STOP.load(Ordering::Relaxed) {
                let result = std::panic::catch_unwind(run_tick);
                if result.is_err() {
                    TICK_ERRORS.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(TICK_INTERVAL_MS));
            }
            RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
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
    fn tick_completes_with_sane_values() {
        let tick = run_tick();
        assert!(tick.tick_id >= 1);
        assert!(tick.semantic_quality >= 0.0 && tick.semantic_quality <= 1.0);
    }

    #[test]
    fn start_stop_does_not_panic() {
        start();
        assert!(is_running());
        stop();
        std::thread::sleep(Duration::from_millis(100));
    }
}
