//! Reasoning synthesis — synthesizes new reasoning chains, routing patterns,
//! optimization strategies, and adaptive cognition paths from existing signals.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

use crate::adaptive_topology::CognitionPath;

pub static CHAINS_SYNTHESIZED: AtomicU64 = AtomicU64::new(0);
pub static CHAINS_REJECTED:    AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY:      usize = 200;
const MIN_CHAIN_LENGTH: usize = 2;
const MAX_CHAIN_LENGTH: usize = 6;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── SynthesizedReasoning ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SynthesizedReasoning {
    pub id:           u64,
    pub label:        String,
    pub confidence:   f32,
    pub path:         CognitionPath,
    pub chain_length: usize,
    pub stability:    f32,
    pub is_valid:     bool,
    pub ts_ms:        u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct SynthStore {
    chains: Vec<SynthesizedReasoning>,
    seq:    u64,
}

impl SynthStore {
    fn new() -> Self { SynthStore { chains: Vec::new(), seq: 0 } }
}

static STORE: Lazy<Mutex<SynthStore>> = Lazy::new(|| Mutex::new(SynthStore::new()));

// ── Synthesis logic ───────────────────────────────────────────────────────────

fn synthesize_chain(
    store:   &mut SynthStore,
    path:    CognitionPath,
    label:   &str,
    base_confidence: f32,
) -> SynthesizedReasoning {
    let stability = (1.0 - crate::adaptive_topology::get_load(path)).clamp(0.0, 1.0);

    // Chain length based on confidence: higher confidence → longer chain
    let chain_length = (MIN_CHAIN_LENGTH
        + (base_confidence * (MAX_CHAIN_LENGTH - MIN_CHAIN_LENGTH) as f32) as usize)
        .min(MAX_CHAIN_LENGTH);

    let verdict = crate::synthesis_validator::validate_synthesis(label, base_confidence, stability);
    let is_valid = verdict.is_valid;
    let confidence = if is_valid { base_confidence } else { base_confidence * 0.5 };

    store.seq += 1;
    let id = store.seq;

    if is_valid {
        CHAINS_SYNTHESIZED.fetch_add(1, Ordering::Relaxed);
    } else {
        CHAINS_REJECTED.fetch_add(1, Ordering::Relaxed);
    }

    SynthesizedReasoning {
        id,
        label: label.to_string(),
        confidence,
        path,
        chain_length,
        stability,
        is_valid,
        ts_ms: ts_now(),
    }
}

/// Synthesize reasoning chains for all cognition paths.
pub fn synthesize_reasoning() -> Vec<SynthesizedReasoning> {
    let unc      = crate::generalized_uncertainty::profile();
    let conf     = crate::confidence_reasoner::assess();
    let sem      = crate::semantic_stability::check();

    let path_specs: &[(CognitionPath, &str, f32)] = &[
        (CognitionPath::Symbolic,      "symbolic_reasoning_chain",
            conf.semantic_reliability * (1.0 - sem.instability_score)),
        (CognitionPath::Probabilistic, "probabilistic_reasoning_chain",
            conf.reasoning_confidence * (1.0 - unc.overall)),
        (CognitionPath::Conceptual,    "conceptual_reasoning_chain",
            conf.planner_confidence * (1.0 - unc.causal_uncertainty)),
        (CognitionPath::Hierarchical,  "hierarchical_reasoning_chain",
            (conf.overall + conf.reasoning_confidence) / 2.0),
    ];

    let mut store = STORE.lock().unwrap();
    let mut chains = Vec::new();

    for (path, label, base_conf) in path_specs {
        let chain = synthesize_chain(&mut store, *path, label, base_conf.clamp(0.1, 0.95));
        if store.chains.len() >= MAX_HISTORY { store.chains.remove(0); }
        store.chains.push(chain.clone());

        crate::world_evolution_observability::record(
            crate::world_evolution_observability::WorldSimEvent::CognitionSynthesized {
                label:      chain.label.clone(),
                confidence: chain.confidence,
            }
        );
        chains.push(chain);
    }

    chains
}

/// Synthesize a single routing pattern for the current optimal path.
pub fn synthesize_routing_pattern() -> SynthesizedReasoning {
    let recommended = crate::adaptive_topology::recommended_path();
    let conf = crate::confidence_reasoner::assess();
    let mut store = STORE.lock().unwrap();
    let chain = synthesize_chain(&mut store, recommended, "routing_pattern", conf.overall);
    if store.chains.len() >= MAX_HISTORY { store.chains.remove(0); }
    store.chains.push(chain.clone());
    chain
}

pub fn valid_chains() -> Vec<SynthesizedReasoning> {
    STORE.lock().unwrap().chains.iter()
        .filter(|c| c.is_valid)
        .cloned()
        .collect()
}

pub fn recent(n: usize) -> Vec<SynthesizedReasoning> {
    STORE.lock().unwrap().chains.iter().rev().take(n).cloned().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_reasoning_non_empty() {
        let chains = synthesize_reasoning();
        assert_eq!(chains.len(), 4);
    }

    #[test]
    fn chain_length_bounded() {
        let chains = synthesize_reasoning();
        for c in &chains {
            assert!(c.chain_length >= MIN_CHAIN_LENGTH && c.chain_length <= MAX_CHAIN_LENGTH);
        }
    }

    #[test]
    fn synthesize_routing_pattern_no_panic() {
        let _ = synthesize_routing_pattern();
    }
}
