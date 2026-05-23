//! Transfer reasoning engine — reuses strategies, recovery logic, and
//! optimization patterns across domains by leveraging analogical mappings.
//! All transfers validated by conceptual_safety; invalid transfers are discarded.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static TRANSFERS_ATTEMPTED: AtomicU64 = AtomicU64::new(0);
pub static TRANSFERS_SUCCEEDED: AtomicU64 = AtomicU64::new(0);
pub static TRANSFERS_FAILED:    AtomicU64 = AtomicU64::new(0);

const MAX_RECORDS: usize = 200;

// ── TransferKind ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TransferKind {
    RecoveryLogic,
    OptimizationLogic,
    StabilizationLogic,
    StrategyReuse,
    PatternReuse,
}

// ── TransferRecord ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransferRecord {
    pub id:           u64,
    pub kind:         TransferKind,
    pub from_context: String,
    pub to_context:   String,
    pub strategy:     String,
    pub success_prob: f32,
    pub validated:    bool,
    pub ts_ms:        u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct TransferState {
    records: Vec<TransferRecord>,
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

fn infer_kind(strategy: &str) -> TransferKind {
    let s = strategy.to_lowercase();
    if s.contains("recover")  { TransferKind::RecoveryLogic }
    else if s.contains("optim") { TransferKind::OptimizationLogic }
    else if s.contains("stabil") { TransferKind::StabilizationLogic }
    else if s.contains("reuse") || s.contains("pattern") { TransferKind::PatternReuse }
    else { TransferKind::StrategyReuse }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Attempt a transfer from one context to another via analogical reasoning.
/// Returns the record if a valid analogy was found, None otherwise.
pub fn attempt_transfer(from_context: impl Into<String>, to_context: impl Into<String>)
    -> Option<TransferRecord>
{
    let from = from_context.into();
    let to   = to_context.into();
    TRANSFERS_ATTEMPTED.fetch_add(1, Ordering::Relaxed);

    let result = crate::analogical_reasoner::apply_analog(&from, &to)?;
    if !result.success {
        TRANSFERS_FAILED.fetch_add(1, Ordering::Relaxed);
        return None;
    }

    // Validate chain depth — count how many transfers already reference `from`
    let chain_depth = STATE.lock()
        .map(|s| s.records.iter().filter(|r| r.to_context == from).count())
        .unwrap_or(0);
    if !crate::conceptual_safety::check_transfer_depth(chain_depth) {
        TRANSFERS_FAILED.fetch_add(1, Ordering::Relaxed);
        return None;
    }

    TRANSFERS_SUCCEEDED.fetch_add(1, Ordering::Relaxed);

    let kind = infer_kind(&result.strategy);
    let success_prob = crate::concept_engine::best_match(&from)
        .map(|c| c.confidence)
        .unwrap_or(0.5);

    let record = if let Ok(mut s) = STATE.lock() {
        if s.records.len() >= MAX_RECORDS { s.records.remove(0); }
        s.seq += 1;
        let r = TransferRecord {
            id: s.seq,
            kind,
            from_context: from.clone(),
            to_context:   to.clone(),
            strategy:     result.strategy.clone(),
            success_prob,
            validated:    true,
            ts_ms:        ts_now(),
        };
        s.records.push(r.clone());
        r
    } else {
        return None;
    };

    // Log to conceptual observability
    crate::conceptual_observability::log(
        crate::conceptual_observability::ConceptualEvent::AnalogicalTransfer {
            from: from.clone(),
            to:   to.clone(),
            strategy: result.strategy,
        }
    );

    Some(record)
}

/// Scan all reliable concepts and attempt pairwise transfers where analogies exist.
/// Returns the number of successful new transfers.
pub fn run_transfer_scan() -> usize {
    let concepts = crate::concept_engine::reliable_concepts();
    let mut count = 0;
    for i in 0..concepts.len() {
        for j in (i + 1)..concepts.len() {
            if attempt_transfer(&concepts[i].label, &concepts[j].label).is_some() {
                count += 1;
            }
        }
    }
    count
}

pub fn recent_transfers(n: usize) -> Vec<TransferRecord> {
    STATE.lock()
        .map(|s| s.records.iter().rev().take(n).cloned().collect())
        .unwrap_or_default()
}

pub fn all_validated() -> Vec<TransferRecord> {
    STATE.lock()
        .map(|s| s.records.iter().filter(|r| r.validated).cloned().collect())
        .unwrap_or_default()
}
