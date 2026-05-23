//! Autonomous cognitive synthesis — synthesizes new reasoning pipelines,
//! generates semantic routing structures, invents cognition optimization
//! strategies, and creates generalized reasoning topologies.
//! All synthesis is bounded, validator-gated, and safety-checked.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static SYNTHESIS_RUNS: AtomicU64 = AtomicU64::new(0);
pub static SYNTHESIS_SAFE: AtomicU64 = AtomicU64::new(0);
pub static SYNTHESIS_GATED: AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 100;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── CognitiveSynthesisResult ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CognitiveSynthesisResult {
    pub tick_id:               u64,
    pub reasoning_structures:  usize,
    pub routing_patterns:      usize,
    pub optimization_strategies: usize,
    pub architectures:         usize,
    pub validated:             bool,
    pub stability_score:       f32,
    pub ts_ms:                 u64,
}

impl CognitiveSynthesisResult {
    pub fn total_outputs(&self) -> usize {
        self.reasoning_structures + self.routing_patterns
            + self.optimization_strategies + self.architectures
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SynthHistory {
    results: Vec<CognitiveSynthesisResult>,
    tick:    u64,
}

impl SynthHistory {
    fn new() -> Self { SynthHistory { results: Vec::new(), tick: 0 } }
}

static HISTORY: Lazy<Mutex<SynthHistory>> = Lazy::new(|| Mutex::new(SynthHistory::new()));

// ── Synthesis entry point ─────────────────────────────────────────────────────

/// Run one synthesis cycle. Gated by simulation safety.
/// Returns a result describing what was synthesized.
pub fn synthesize() -> CognitiveSynthesisResult {
    SYNTHESIS_RUNS.fetch_add(1, Ordering::Relaxed);

    let mut h = HISTORY.lock().unwrap();
    h.tick += 1;
    let tick_id = h.tick;
    drop(h);

    // Safety gate — abort early if synthesis is unsafe
    let safety = crate::simulation_safety::check_synthesis_safe();
    if !safety.is_safe {
        SYNTHESIS_GATED.fetch_add(1, Ordering::Relaxed);
        let result = CognitiveSynthesisResult {
            tick_id,
            reasoning_structures: 0,
            routing_patterns: 0,
            optimization_strategies: 0,
            architectures: 0,
            validated: false,
            stability_score: 0.0,
            ts_ms: ts_now(),
        };
        crate::world_evolution_observability::record(
            crate::world_evolution_observability::WorldSimEvent::SafetyIntervention {
                component: "autonomous_synthesis".into(),
                reason: safety.reason.unwrap_or_else(|| "safety_check_failed".into()),
            }
        );
        push_result(result.clone());
        return result;
    }

    // 1. Synthesize reasoning chains
    let chains = crate::reasoning_synthesis::synthesize_reasoning();
    let reasoning_structures = chains.iter().filter(|c| c.is_valid).count();

    // 2. Synthesize routing pattern
    let _routing = crate::reasoning_synthesis::synthesize_routing_pattern();
    let routing_patterns = if _routing.is_valid { 1 } else { 0 };

    // 3. Generate semantic architecture (one per cycle)
    let arch = crate::semantic_architecture_generator::generate_architecture();
    let architectures = if arch.is_stable { 1 } else { 0 };

    // 4. Optimization strategies — use topology generation as strategy source
    let candidates = crate::topology_generation::generate_topology_candidates(2);
    let optimization_strategies = candidates.iter().filter(|t| t.is_valid).count();

    // Compute stability score from components
    let conf = crate::confidence_reasoner::assess();
    let stability_score = (conf.overall
        + arch.semantic_coherence
        + reasoning_structures as f32 / (chains.len().max(1) as f32)) / 3.0;

    let validated = reasoning_structures > 0 || architectures > 0;

    SYNTHESIS_SAFE.fetch_add(1, Ordering::Relaxed);

    let result = CognitiveSynthesisResult {
        tick_id,
        reasoning_structures,
        routing_patterns,
        optimization_strategies,
        architectures,
        validated,
        stability_score: stability_score.clamp(0.0, 1.0),
        ts_ms: ts_now(),
    };

    // Store to future_memory
    crate::future_memory::store(
        crate::future_memory::FutureCategory::SyntheticCognition,
        format!(
            "tick={tick_id}_chains={reasoning_structures}_arch={architectures}"
        ),
        1.0 - stability_score,
    );

    push_result(result.clone());
    result
}

fn push_result(r: CognitiveSynthesisResult) {
    let mut h = HISTORY.lock().unwrap();
    if h.results.len() >= MAX_HISTORY { h.results.remove(0); }
    h.results.push(r);
}

pub fn recent(n: usize) -> Vec<CognitiveSynthesisResult> {
    HISTORY.lock().unwrap().results.iter().rev().take(n).cloned().collect()
}

pub fn latest() -> Option<CognitiveSynthesisResult> {
    HISTORY.lock().unwrap().results.last().cloned()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_no_panic() {
        let r = synthesize();
        assert!(r.stability_score >= 0.0 && r.stability_score <= 1.0);
    }

    #[test]
    fn synthesize_increments_counter() {
        let before = SYNTHESIS_RUNS.load(Ordering::Relaxed);
        let _ = synthesize();
        assert!(SYNTHESIS_RUNS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn total_outputs_non_negative() {
        let r = synthesize();
        assert!(r.total_outputs() < 100); // sanity bound
    }
}
