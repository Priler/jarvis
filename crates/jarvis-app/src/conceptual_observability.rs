//! Conceptual observability — typed log for abstraction engine events.
//! Logs concept creation, abstraction evolution, analogical transfers,
//! conceptual conflicts, generalization quality, and transfer failures.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_LOG: usize = 500;

// ── ConceptualEvent ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ConceptualEvent {
    ConceptCreated { label: String, kind: String, confidence: f32 },
    AbstractionEvolved { label: String, old_level: u8, new_level: u8 },
    AnalogicalTransfer { from: String, to: String, strategy: String },
    ConceptualConflict { concept_a: String, concept_b: String, reason: String },
    GeneralizationQuality { quality: f32, concept_count: usize, strong_count: usize },
    TransferFailure { from: String, to: String, reason: String },
    WorldModelUpdate { state: String, fragility: f32 },
    AbstractionTick { tick_id: u64, concepts: usize, transfers: usize, healthy: bool },
}

impl ConceptualEvent {
    pub fn severity(&self) -> &'static str {
        match self {
            ConceptualEvent::ConceptualConflict { .. }  => "WARN",
            ConceptualEvent::TransferFailure { .. }     => "WARN",
            ConceptualEvent::GeneralizationQuality { quality, .. } if *quality < 0.4 => "WARN",
            ConceptualEvent::AbstractionTick { healthy: false, .. } => "WARN",
            _ => "INFO",
        }
    }
}

// ── Log entry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConceptualLogEntry {
    pub id:       u64,
    pub event:    ConceptualEvent,
    pub severity: String,
    pub ts_ms:    u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ObsState {
    log: Vec<ConceptualLogEntry>,
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

pub fn log(event: ConceptualEvent) {
    if let Ok(mut s) = STATE.lock() {
        s.seq += 1;
        let id = s.seq;
        let severity = event.severity().to_string();
        if s.log.len() >= MAX_LOG { s.log.remove(0); }
        s.log.push(ConceptualLogEntry { id, severity, event, ts_ms: ts_now() });
    }
}

pub fn recent(n: usize) -> Vec<ConceptualLogEntry> {
    STATE.lock()
        .map(|s| s.log.iter().rev().take(n).cloned().collect())
        .unwrap_or_default()
}

pub fn snapshot() -> Vec<ConceptualLogEntry> {
    STATE.lock().map(|s| s.log.clone()).unwrap_or_default()
}

pub fn warn_count() -> usize {
    STATE.lock()
        .map(|s| s.log.iter().filter(|e| e.severity == "WARN").count())
        .unwrap_or(0)
}
