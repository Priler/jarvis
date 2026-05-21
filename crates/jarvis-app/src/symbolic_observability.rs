//! Symbolic observability — typed rolling log for all Phase 21 events.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_LOG: usize = 500;

// ── SymbolicEvent ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SymbolicEvent {
    InferenceChainBuilt  { root: String, conclusion: String, depth: usize, confidence: f32 },
    ContradictionDetected { entity_a: String, entity_b: String, severity: f32 },
    ContradictionResolved { entity_a: String, entity_b: String, resolution: String },
    ConceptSynthesized   { label: String, sources: Vec<String>, confidence: f32 },
    ConstraintViolated   { kind: String, severity: f32 },
    SemanticTransfer     { from: String, to: String, chain_depth: usize },
    StabilityCheck       { is_stable: bool, reason: String },
    SymbolicTick         { tick_id: u64, chains: usize, contradictions: usize,
                           syntheses: usize, healthy: bool },
}

impl SymbolicEvent {
    pub fn severity(&self) -> &'static str {
        match self {
            SymbolicEvent::ContradictionDetected { severity, .. } if *severity > 0.7 => "CRIT",
            SymbolicEvent::ContradictionDetected { .. }  => "WARN",
            SymbolicEvent::ConstraintViolated { .. }     => "WARN",
            SymbolicEvent::StabilityCheck { is_stable: false, .. } => "WARN",
            SymbolicEvent::SymbolicTick { healthy: false, .. }     => "WARN",
            _ => "INFO",
        }
    }
}

// ── Log entry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolicLogEntry {
    pub id:       u64,
    pub event:    SymbolicEvent,
    pub severity: String,
    pub ts_ms:    u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ObsState {
    log: Vec<SymbolicLogEntry>,
    seq: u64,
}

static STATE: Lazy<Mutex<ObsState>> = Lazy::new(|| Mutex::new(ObsState {
    log: Vec::new(),
    seq: 0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn log(event: SymbolicEvent) {
    if let Ok(mut s) = STATE.lock() {
        s.seq += 1;
        let id = s.seq;
        let severity = event.severity().to_string();
        if s.log.len() >= MAX_LOG { s.log.remove(0); }
        s.log.push(SymbolicLogEntry { id, severity, event, ts_ms: ts_now() });
    }
}

pub fn recent(n: usize) -> Vec<SymbolicLogEntry> {
    STATE.lock()
        .map(|s| s.log.iter().rev().take(n).cloned().collect())
        .unwrap_or_default()
}

pub fn snapshot() -> Vec<SymbolicLogEntry> {
    STATE.lock().map(|s| s.log.clone()).unwrap_or_default()
}

pub fn warn_count() -> usize {
    STATE.lock()
        .map(|s| s.log.iter().filter(|e| e.severity != "INFO").count())
        .unwrap_or(0)
}
