//! Semantic contradiction engine — detects and resolves mutually incompatible
//! beliefs in the semantic graph. Prevents contradiction cascades.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static CONTRADICTIONS_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static CONTRADICTIONS_RESOLVED: AtomicU64 = AtomicU64::new(0);

const MAX_HISTORY: usize = 300;

// ── ContradictionKind ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ContradictionKind {
    ConflictingStrategies,
    InvalidAbstraction,
    CausalInconsistency,
    PlannerContradiction,
    ResourceParadox,
    GoalConflict,
}

// ── Contradiction ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Contradiction {
    pub id:            u64,
    pub kind:          ContradictionKind,
    pub entity_a:      String,
    pub entity_b:      String,
    pub severity:      f32,        // 0–1
    pub resolved:      bool,
    pub resolution:    Option<String>,
    pub ts_ms:         u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ContradictionState {
    history: Vec<Contradiction>,
    seq:     u64,
}

static STATE: Lazy<Mutex<ContradictionState>> = Lazy::new(|| Mutex::new(ContradictionState {
    history: Vec::new(),
    seq:     0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn classify_kind(label_a: &str, label_b: &str) -> ContradictionKind {
    let a = label_a.to_lowercase();
    let b = label_b.to_lowercase();
    if a.contains("strateg") || b.contains("strateg") { ContradictionKind::ConflictingStrategies }
    else if a.contains("goal") || b.contains("goal")   { ContradictionKind::GoalConflict }
    else if a.contains("plan") || b.contains("plan")   { ContradictionKind::PlannerContradiction }
    else if a.contains("resource") || b.contains("resource") { ContradictionKind::ResourceParadox }
    else if a.contains("caus") || b.contains("caus")   { ContradictionKind::CausalInconsistency }
    else { ContradictionKind::InvalidAbstraction }
}

// ── Detection ─────────────────────────────────────────────────────────────────

/// Scan the semantic graph for contradiction edges and return all found.
pub fn detect() -> Vec<Contradiction> {
    let edges = crate::semantic_graph::contradiction_edges();
    let mut found = Vec::new();
    let now = ts_now();

    for edge in &edges {
        let entity_a = match crate::semantic_graph::get_entity(edge.from) {
            Some(e) => e, None => continue,
        };
        let entity_b = match crate::semantic_graph::get_entity(edge.to) {
            Some(e) => e, None => continue,
        };

        // Check if this contradiction is already recorded (unresolved)
        let already_known = STATE.lock()
            .map(|s| s.history.iter().any(|c|
                !c.resolved &&
                ((c.entity_a == entity_a.label && c.entity_b == entity_b.label)
                 || (c.entity_a == entity_b.label && c.entity_b == entity_a.label))
            ))
            .unwrap_or(false);

        if already_known { continue; }

        CONTRADICTIONS_DETECTED.fetch_add(1, Ordering::Relaxed);
        let kind = classify_kind(&entity_a.label, &entity_b.label);
        let severity = edge.weight;

        let c = if let Ok(mut s) = STATE.lock() {
            if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
            s.seq += 1;
            let id = s.seq;
            let c = Contradiction {
                id, kind, severity,
                entity_a: entity_a.label.clone(),
                entity_b: entity_b.label.clone(),
                resolved: false,
                resolution: None,
                ts_ms: now,
            };
            s.history.push(c.clone());
            c
        } else { continue };

        found.push(c);
    }

    // Check for cascade
    if crate::symbolic_safety::check_contradiction_cascade(found.len()) {
        crate::symbolic_observability::log(
            crate::symbolic_observability::SymbolicEvent::StabilityCheck {
                is_stable: false,
                reason: "contradiction_cascade_detected".to_string(),
            }
        );
    }

    found
}

/// Resolve a contradiction: remove the lower-confidence entity from the graph
/// and mark the contradiction as resolved.
pub fn resolve(contradiction_id: u64) {
    let c = STATE.lock()
        .ok()
        .and_then(|s| s.history.iter().find(|c| c.id == contradiction_id).cloned());

    if let Some(c) = c {
        // Resolve by retaining higher-confidence entity
        let a_conf = crate::semantic_graph::entity_id(&c.entity_a)
            .and_then(crate::semantic_graph::get_entity)
            .map(|e| e.confidence)
            .unwrap_or(0.0);
        let b_conf = crate::semantic_graph::entity_id(&c.entity_b)
            .and_then(crate::semantic_graph::get_entity)
            .map(|e| e.confidence)
            .unwrap_or(0.0);
        let resolution = if a_conf >= b_conf {
            format!("retained:{} discarded:{}", c.entity_a, c.entity_b)
        } else {
            format!("retained:{} discarded:{}", c.entity_b, c.entity_a)
        };

        if let Ok(mut s) = STATE.lock() {
            if let Some(cr) = s.history.iter_mut().find(|c| c.id == contradiction_id) {
                cr.resolved   = true;
                cr.resolution = Some(resolution);
            }
        }
        CONTRADICTIONS_RESOLVED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn active_contradictions() -> Vec<Contradiction> {
    STATE.lock()
        .map(|s| s.history.iter().filter(|c| !c.resolved).cloned().collect())
        .unwrap_or_default()
}

pub fn history() -> Vec<Contradiction> {
    STATE.lock().map(|s| s.history.clone()).unwrap_or_default()
}
