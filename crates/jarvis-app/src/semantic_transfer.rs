//! Semantic transfer — transfers symbolic structures and inference chains
//! across domains by mapping symbolic patterns between contexts.
//! Validated by symbolic_safety before application.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static TRANSFERS_ATTEMPTED: AtomicU64 = AtomicU64::new(0);
pub static TRANSFERS_COMPLETED: AtomicU64 = AtomicU64::new(0);

const MAX_RECORDS: usize = 200;

// ── TransferredChain ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransferredChain {
    pub id:            u64,
    pub source_root:   String,
    pub target_domain: String,
    pub chain_depth:   usize,
    pub confidence:    f32,
    pub strategy:      String,
    pub ts_ms:         u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct TransferState {
    records: Vec<TransferredChain>,
    seq:     u64,
}

static STATE: Lazy<Mutex<TransferState>> = Lazy::new(|| Mutex::new(TransferState {
    records: Vec::new(),
    seq:     0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Transfer a symbolic inference chain to a target domain.
/// Validates safety (depth, confidence) before storing the transfer.
pub fn transfer_chain(
    source_root: impl Into<String>,
    target_domain: impl Into<String>,
) -> Option<TransferredChain> {
    let source = source_root.into();
    let target = target_domain.into();
    TRANSFERS_ATTEMPTED.fetch_add(1, Ordering::Relaxed);

    // Circular check
    if crate::symbolic_safety::is_circular(&source, &target) { return None; }

    // Get the existing forward chain from source
    let chains = crate::symbolic_inference::reliable_chains();
    let chain = chains.iter().find(|c| c.root == source)?;

    // Safety validation
    let verdict = crate::symbolic_safety::validate_chain(chain.depth, chain.confidence);
    if !verdict.is_valid() { return None; }

    // Derive transfer strategy from chain conclusion
    let strategy = format!("apply_{}_strategy_in_{}", chain.conclusion, target)
        .to_lowercase().replace(' ', "_");

    // Register the transfer in the semantic graph
    crate::semantic_graph::relate(
        &source, crate::semantic_graph::EntityKind::Concept,
        &target, crate::semantic_graph::EntityKind::Concept,
        crate::semantic_graph::SemanticRelation::Inferred,
        chain.confidence,
    );

    TRANSFERS_COMPLETED.fetch_add(1, Ordering::Relaxed);

    let record = if let Ok(mut s) = STATE.lock() {
        if s.records.len() >= MAX_RECORDS { s.records.remove(0); }
        s.seq += 1;
        let id = s.seq;
        let r = TransferredChain {
            id, source_root: source.clone(), target_domain: target.clone(),
            chain_depth: chain.depth, confidence: chain.confidence,
            strategy: strategy.clone(), ts_ms: ts_now(),
        };
        s.records.push(r.clone());
        r
    } else { return None; };

    crate::symbolic_observability::log(
        crate::symbolic_observability::SymbolicEvent::SemanticTransfer {
            from:        source,
            to:          target,
            chain_depth: record.chain_depth,
        }
    );

    Some(record)
}

pub fn recent_transfers(n: usize) -> Vec<TransferredChain> {
    STATE.lock()
        .map(|s| s.records.iter().rev().take(n).cloned().collect())
        .unwrap_or_default()
}

pub fn all_transfers() -> Vec<TransferredChain> {
    STATE.lock().map(|s| s.records.clone()).unwrap_or_default()
}
