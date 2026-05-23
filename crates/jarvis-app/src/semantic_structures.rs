//! Semantic structure detector — finds recurring forms in operational observations.
//! Structures are workflow archetypes, strategic motifs, failure patterns, and
//! optimization templates. No ML; frequency + recency weighted heuristics.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static STRUCTURES_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static STRUCTURES_STRONG:   AtomicU64 = AtomicU64::new(0);

const MAX_STRUCTURES:   usize = 200;
const STRONG_THRESHOLD: u32   = 4;   // occurrences to be "strong"
const MIN_CONFIDENCE:   f32   = 0.4;

// ── StructureKind ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StructureKind {
    RecurringFailure,
    RecurringWorkflow,
    RecurringStrategy,
    RecurringOptimization,
    EnvironmentPattern,
    StrategicMotif,
}

// ── SemanticStructure ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticStructure {
    pub id:          u64,
    pub kind:        StructureKind,
    pub label:       String,
    pub occurrences: u32,
    pub confidence:  f32,
    pub last_seen_ms: u64,
}

impl SemanticStructure {
    pub fn is_strong(&self) -> bool {
        self.occurrences >= STRONG_THRESHOLD && self.confidence >= MIN_CONFIDENCE
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct StructureState {
    structures: Vec<SemanticStructure>,
    freq:       HashMap<String, u32>,
    seq:        u64,
}

static STATE: Lazy<Mutex<StructureState>> = Lazy::new(|| Mutex::new(StructureState {
    structures: Vec::new(),
    freq:       HashMap::new(),
    seq:        0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn normalize_label(label: &str, kind: &StructureKind) -> String {
    let prefix = match kind {
        StructureKind::RecurringFailure      => "failure",
        StructureKind::RecurringWorkflow     => "workflow",
        StructureKind::RecurringStrategy     => "strategy",
        StructureKind::RecurringOptimization => "optimization",
        StructureKind::EnvironmentPattern    => "env",
        StructureKind::StrategicMotif        => "motif",
    };
    format!("{prefix}::{}", label.to_lowercase().replace(' ', "_"))
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Record an occurrence of a labelled structure.
pub fn record(label: impl Into<String>, kind: StructureKind) {
    let label = label.into();
    let key = normalize_label(&label, &kind);
    let now = ts_now();

    if let Ok(mut s) = STATE.lock() {
        let count = s.freq.entry(key.clone()).or_insert(0);
        *count += 1;
        let c = *count;

        if let Some(existing) = s.structures.iter_mut().find(|st| st.label == key) {
            existing.occurrences  = c;
            existing.confidence   = (c as f32 / 10.0).min(1.0);
            existing.last_seen_ms = now;
            if existing.is_strong() { STRUCTURES_STRONG.fetch_add(1, Ordering::Relaxed); }
        } else {
            if s.structures.len() >= MAX_STRUCTURES { s.structures.remove(0); }
            s.seq += 1;
            let struct_id = s.seq;
            s.structures.push(SemanticStructure {
                id: struct_id, kind, label: key,
                occurrences: c,
                confidence: (c as f32 / 10.0).min(1.0),
                last_seen_ms: now,
            });
            STRUCTURES_DETECTED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn strong_structures() -> Vec<SemanticStructure> {
    STATE.lock()
        .map(|s| s.structures.iter().filter(|st| st.is_strong()).cloned().collect())
        .unwrap_or_default()
}

pub fn snapshot() -> Vec<SemanticStructure> {
    STATE.lock().map(|s| s.structures.clone()).unwrap_or_default()
}

pub fn by_kind(kind: &StructureKind) -> Vec<SemanticStructure> {
    STATE.lock()
        .map(|s| s.structures.iter().filter(|st| &st.kind == kind).cloned().collect())
        .unwrap_or_default()
}
