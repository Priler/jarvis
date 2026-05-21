//! Abstraction runtime — central Phase 20 coordinator.
//! Runs a background thread "jarvis-abstraction-runtime" at 2000ms cadence.
//! 8-step tick: safety → concept_engine → semantic_structures → abstraction_graph
//!              → analogical_reasoning → world_model → transfer_scan → observability.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use once_cell::sync::Lazy;

pub static TICKS_TOTAL:   AtomicU64 = AtomicU64::new(0);
pub static TICK_ERRORS:   AtomicU64 = AtomicU64::new(0);
static RUNNING: AtomicBool = AtomicBool::new(false);
static STOP:    AtomicBool = AtomicBool::new(false);

const TICK_INTERVAL_MS: u64 = 2000;

// ── Tick result ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AbstractionTick {
    pub tick_id:            u64,
    pub concepts_active:    usize,
    pub analogies_found:    usize,
    pub transfers_this_tick: usize,
    pub world_state:        String,
    pub quality:            f32,
    pub safety_ok:          bool,
    pub healthy:            bool,
    pub duration_ms:        u64,
}

// ── Tick implementation ───────────────────────────────────────────────────────

pub fn run_tick() -> AbstractionTick {
    let tick_id = TICKS_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    let start_ms = ts_now();

    // Step 1 — Safety check: validate pending concepts
    let safety_ok = {
        let pending = crate::concept_engine::snapshot();
        let rejected = pending.iter()
            .filter(|c| !crate::conceptual_safety::validate_concept(c).is_valid())
            .count();
        rejected == 0 || rejected < pending.len() / 2
    };

    // Step 2 — Feed causal observations into concept engine
    {
        let links = crate::causal_reasoner::reliable_links();
        for link in links.iter().take(20) {
            crate::concept_engine::observe(&link.cause);
            crate::concept_engine::observe(&link.effect);
        }
    }

    // Step 3 — Feed workflow patterns into semantic structures
    {
        let patterns = crate::workflow_learning::strong_patterns();
        for p in patterns.iter().take(10) {
            let label = p.sequence.join("_workflow_step_");
            crate::semantic_structures::record(
                &label, crate::semantic_structures::StructureKind::RecurringWorkflow);
        }
    }

    // Step 4 — Conceptual reasoning (populates abstraction graph)
    let concept_result = crate::conceptual_reasoner::reason();
    let concepts_active = concept_result.concepts_active;

    // Step 5 — Analogical reasoning
    let analogies_found = {
        let reliable = crate::concept_engine::reliable_concepts();
        let mut total = 0;
        for c in reliable.iter().take(5) {
            total += crate::analogical_reasoner::find_analogies(&c.label).len();
        }
        total
    };

    // Step 6 — World model update
    let world_snap = crate::conceptual_world_model::update();
    let world_state = format!("{:?}", world_snap.abstract_state);

    // Step 7 — Transfer scan (bounded: max 10 pairs per tick to avoid explosion)
    let transfers_this_tick = {
        let concepts = crate::concept_engine::reliable_concepts();
        let mut count = 0;
        let limit = concepts.len().min(5);
        for i in 0..limit {
            for j in (i + 1)..limit {
                if crate::transfer_reasoning::attempt_transfer(
                    &concepts[i].label, &concepts[j].label).is_some() {
                    count += 1;
                }
            }
        }
        count
    };

    // Step 8 — Generalized reasoning
    let gen_result = crate::generalized_reasoner::reason();
    let quality = (concept_result.quality + gen_result.abstraction_quality) / 2.0;
    let healthy  = safety_ok && gen_result.healthy;

    // Step 9 — Abstract goals tick
    crate::abstract_goals::tick_progress(0.005);

    // Step 10 — Abstract resource reasoning
    let _res_snap = crate::abstract_resource_reasoner::sample();

    // Step 11 — Log tick to observability
    crate::conceptual_observability::log(
        crate::conceptual_observability::ConceptualEvent::AbstractionTick {
            tick_id,
            concepts: concepts_active,
            transfers: transfers_this_tick,
            healthy,
        }
    );

    let duration_ms = ts_now().saturating_sub(start_ms);
    AbstractionTick {
        tick_id, concepts_active, analogies_found, transfers_this_tick,
        world_state, quality, safety_ok, healthy, duration_ms,
    }
}

// ── Background thread ─────────────────────────────────────────────────────────

pub fn start() {
    if RUNNING.swap(true, Ordering::SeqCst) { return; }
    STOP.store(false, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("jarvis-abstraction-runtime".to_string())
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
    fn tick_completes_and_is_sane() {
        let tick = run_tick();
        assert!(tick.tick_id >= 1);
        assert!(tick.quality >= 0.0 && tick.quality <= 1.0);
    }

    #[test]
    fn start_stop_cycle() {
        start();
        assert!(is_running());
        stop();
        // Give thread time to exit
        std::thread::sleep(Duration::from_millis(100));
    }
}
