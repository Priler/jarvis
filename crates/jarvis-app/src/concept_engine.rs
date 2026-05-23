//! Core concept engine — creates, groups, and generalizes operational concepts.
//! Concepts are inferred from labelled observations; no ML, no cloud.
//! Grouping is frequency-based: labels sharing a prefix or semantic marker
//! cluster into a generalized concept after MIN_CLUSTER_SIZE observations.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static CONCEPTS_CREATED:   AtomicU64 = AtomicU64::new(0);
pub static CONCEPTS_GENERALIZED: AtomicU64 = AtomicU64::new(0);
pub static OBSERVATIONS_TOTAL: AtomicU64 = AtomicU64::new(0);

const MAX_CONCEPTS:      usize = 500;
const MAX_OBSERVATIONS:  usize = 2000;
const MIN_CLUSTER_SIZE:  u32   = 3;   // observations needed before generalization
const MIN_CONFIDENCE:    f32   = 0.35; // below this a concept is tentative

// ── ConceptKind ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ConceptKind {
    Workflow,
    Goal,
    Constraint,
    Resource,
    Risk,
    Dependency,
    Optimization,
    Failure,
    Strategy,
    Unknown,
}

impl ConceptKind {
    pub fn from_label(label: &str) -> Self {
        let l = label.to_lowercase();
        if l.contains("fail") || l.contains("error") || l.contains("crash") { ConceptKind::Failure }
        else if l.contains("workflow") || l.contains("task") || l.contains("job") { ConceptKind::Workflow }
        else if l.contains("goal") || l.contains("objective") { ConceptKind::Goal }
        else if l.contains("resource") || l.contains("cpu") || l.contains("memory") { ConceptKind::Resource }
        else if l.contains("risk") || l.contains("unstable") || l.contains("fragil") { ConceptKind::Risk }
        else if l.contains("optim") || l.contains("improve") { ConceptKind::Optimization }
        else if l.contains("strateg") || l.contains("plan") { ConceptKind::Strategy }
        else if l.contains("depend") || l.contains("requir") { ConceptKind::Dependency }
        else if l.contains("constraint") || l.contains("limit") { ConceptKind::Constraint }
        else { ConceptKind::Unknown }
    }
}

// ── Concept ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Concept {
    pub id:               u64,
    pub kind:             ConceptKind,
    pub label:            String,         // generalized label
    pub abstraction_level: u8,            // 0 = concrete, 1-3 = increasing abstraction
    pub examples:         Vec<String>,    // concrete observations that produced this
    pub confidence:       f32,            // 0–1; rises with more corroborating observations
    pub observation_count: u32,
    pub ts_ms:            u64,
}

impl Concept {
    pub fn is_reliable(&self) -> bool {
        self.confidence >= MIN_CONFIDENCE && self.observation_count >= MIN_CLUSTER_SIZE
    }
}

// ── Raw observation ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RawObservation {
    label:   String,
    kind:    ConceptKind,
    _ts_ms:  u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct EngineState {
    concepts:     Vec<Concept>,
    observations: Vec<RawObservation>,
    concept_seq:  u64,
    // cluster counters: normalized_key → count
    cluster_counts: HashMap<String, u32>,
}

static STATE: Lazy<Mutex<EngineState>> = Lazy::new(|| Mutex::new(EngineState {
    concepts:       Vec::new(),
    observations:   Vec::new(),
    concept_seq:    0,
    cluster_counts: HashMap::new(),
}));

// ── Internal helpers ──────────────────────────────────────────────────────────

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Normalize a label into a cluster key by stripping specific identifiers.
/// "docker failure", "browser launch failure", "IDE startup failure" → "initialization_failure"
fn cluster_key(label: &str) -> String {
    let l = label.to_lowercase();
    // Identify semantic markers
    let markers = [
        ("fail", "failure"),
        ("error", "failure"),
        ("crash", "failure"),
        ("unstable", "instability"),
        ("timeout", "timeout"),
        ("starvat", "starvation"),
        ("overload", "overload"),
        ("optim", "optimization"),
        ("recover", "recovery"),
        ("strateg", "strategy"),
        ("init", "initialization"),
        ("launch", "initialization"),
        ("startup", "initialization"),
        ("load", "load"),
    ];
    for (needle, replacement) in &markers {
        if l.contains(needle) {
            return replacement.to_string();
        }
    }
    // Fallback: take first word
    l.split_whitespace().next().unwrap_or("unknown").to_string()
}

/// Build a generalized label from a cluster key and kind.
fn generalize_label(cluster_key: &str, kind: &ConceptKind) -> String {
    match (cluster_key, kind) {
        ("initialization", ConceptKind::Failure) => "environment_initialization_instability".to_string(),
        ("failure",        ConceptKind::Failure) => "generalized_operational_failure".to_string(),
        ("instability",    ConceptKind::Risk)    => "systemic_instability".to_string(),
        ("starvation",     ConceptKind::Resource)=> "resource_starvation".to_string(),
        ("overload",       ConceptKind::Resource)=> "resource_overload".to_string(),
        ("recovery",       ConceptKind::Strategy)=> "generalized_recovery_strategy".to_string(),
        ("optimization",   ConceptKind::Optimization) => "generalized_optimization".to_string(),
        ("timeout",        ConceptKind::Failure) => "timeout_pattern".to_string(),
        ("load",           ConceptKind::Resource)=> "load_pressure".to_string(),
        _ => format!("{}_{}", cluster_key, format!("{:?}", kind).to_lowercase()),
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Record a labelled observation. May trigger concept creation or reinforcement.
pub fn observe(label: impl Into<String>) {
    let label = label.into();
    let kind = ConceptKind::from_label(&label);
    let key = cluster_key(&label);
    let now = ts_now();

    OBSERVATIONS_TOTAL.fetch_add(1, Ordering::Relaxed);

    if let Ok(mut s) = STATE.lock() {
        // Evict oldest observations if at capacity
        if s.observations.len() >= MAX_OBSERVATIONS {
            s.observations.remove(0);
        }
        s.observations.push(RawObservation { label: label.clone(), kind: kind.clone(), _ts_ms: now });

        // Increment cluster counter
        let count = s.cluster_counts.entry(key.clone()).or_insert(0);
        *count += 1;
        let count_val = *count;

        // Check if we should create or reinforce a concept
        if count_val >= MIN_CLUSTER_SIZE {
            let gen_label = generalize_label(&key, &kind);
            // Look for existing concept with this generalized label
            if let Some(existing) = s.concepts.iter_mut().find(|c| c.label == gen_label) {
                existing.observation_count += 1;
                existing.confidence = (existing.confidence + 0.05).min(1.0);
                existing.examples.push(label.clone());
                if existing.examples.len() > 20 { existing.examples.remove(0); }
                existing.ts_ms = now;
            } else {
                // Create new concept
                if s.concepts.len() >= MAX_CONCEPTS { s.concepts.remove(0); }
                s.concept_seq += 1;
                let id = s.concept_seq;
                let abstraction_level = match count_val {
                    3..=9  => 1,
                    10..=29 => 2,
                    _       => 3,
                };
                s.concepts.push(Concept {
                    id,
                    kind,
                    label: gen_label,
                    abstraction_level,
                    examples: vec![label],
                    confidence: (count_val as f32 / 10.0).min(1.0),
                    observation_count: count_val,
                    ts_ms: now,
                });
                CONCEPTS_CREATED.fetch_add(1, Ordering::Relaxed);
                if abstraction_level >= 2 { CONCEPTS_GENERALIZED.fetch_add(1, Ordering::Relaxed); }
            }
        }
    }
}

/// Get all reliable concepts (confidence ≥ MIN_CONFIDENCE, obs ≥ MIN_CLUSTER_SIZE).
pub fn reliable_concepts() -> Vec<Concept> {
    STATE.lock()
        .map(|s| s.concepts.iter().filter(|c| c.is_reliable()).cloned().collect())
        .unwrap_or_default()
}

/// Get all concepts, including tentative ones.
pub fn snapshot() -> Vec<Concept> {
    STATE.lock()
        .map(|s| s.concepts.clone())
        .unwrap_or_default()
}

/// Get concept by id.
pub fn get(id: u64) -> Option<Concept> {
    STATE.lock()
        .ok()
        .and_then(|s| s.concepts.iter().find(|c| c.id == id).cloned())
}

/// Find concepts of a given kind.
pub fn by_kind(kind: &ConceptKind) -> Vec<Concept> {
    STATE.lock()
        .map(|s| s.concepts.iter().filter(|c| &c.kind == kind).cloned().collect())
        .unwrap_or_default()
}

/// Find the most confident concept for a given label observation.
pub fn best_match(label: &str) -> Option<Concept> {
    let key = cluster_key(label);
    let kind = ConceptKind::from_label(label);
    let gen_label = generalize_label(&key, &kind);
    STATE.lock()
        .ok()
        .and_then(|s| s.concepts.iter().find(|c| c.label == gen_label).cloned())
}
