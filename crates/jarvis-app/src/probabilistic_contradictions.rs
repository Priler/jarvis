//! Probabilistic contradiction resolution — resolves conflicting beliefs,
//! weak evidence conflicts, and uncertain causal chains.

use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

pub static CONFLICTS_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static CONFLICTS_RESOLVED: AtomicU64 = AtomicU64::new(0);

const MAX_CONFLICTS: usize = 200;
const WEAK_THRESHOLD: f32  = 0.40;  // both sides weak evidence → weak conflict

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── ProbabilisticConflict ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProbabilisticConflict {
    pub id:                   u64,
    pub belief_a:             String,
    pub belief_b:             String,
    pub conflict_weight:      f32,   // 0–1, how severe
    pub resolution_confidence: f32,
    pub resolved:             bool,
    pub winner:               Option<String>,
    pub ts_ms:                u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct ConflictState {
    conflicts: Vec<ProbabilisticConflict>,
    seq:       u64,
}

static STATE: Lazy<Mutex<ConflictState>> = Lazy::new(|| Mutex::new(ConflictState {
    conflicts: Vec::new(),
    seq:       0,
}));

// ── Detection ─────────────────────────────────────────────────────────────────

pub fn detect_conflicts() -> Vec<ProbabilisticConflict> {
    // Pull active semantic contradictions and convert to probabilistic conflicts
    let contradictions = crate::semantic_contradictions::active_contradictions();
    let beliefs        = crate::belief_engine::all_beliefs();

    let mut new_conflicts = Vec::new();
    for c in contradictions {
        // Avoid duplicates
        let already = {
            let s = STATE.lock().unwrap();
            s.conflicts.iter().any(|cf|
                (cf.belief_a == c.entity_a && cf.belief_b == c.entity_b) ||
                (cf.belief_a == c.entity_b && cf.belief_b == c.entity_a))
        };
        if already { continue; }

        let conf_a = beliefs.iter().find(|b| b.label == c.entity_a)
            .map(|b| b.effective_confidence()).unwrap_or(0.30);
        let conf_b = beliefs.iter().find(|b| b.label == c.entity_b)
            .map(|b| b.effective_confidence()).unwrap_or(0.30);
        let conflict_weight = c.severity * 0.70 + (1.0 - (conf_a - conf_b).abs()) * 0.30;
        let resolution_confidence = (conf_a - conf_b).abs().clamp(0.0, 1.0);

        let mut s = STATE.lock().unwrap();
        s.seq += 1;
        let id = s.seq;
        let conflict = ProbabilisticConflict {
            id,
            belief_a:             c.entity_a.clone(),
            belief_b:             c.entity_b.clone(),
            conflict_weight:      conflict_weight.clamp(0.0, 1.0),
            resolution_confidence,
            resolved:             false,
            winner:               None,
            ts_ms:                ts_now(),
        };
        if s.conflicts.len() >= MAX_CONFLICTS { s.conflicts.remove(0); }
        s.conflicts.push(conflict.clone());
        new_conflicts.push(conflict);
        CONFLICTS_DETECTED.fetch_add(1, Ordering::Relaxed);
    }
    new_conflicts
}

pub fn resolve_by_confidence() -> usize {
    let beliefs = crate::belief_engine::all_beliefs();
    let mut s   = STATE.lock().unwrap();
    let mut resolved = 0;
    for cf in s.conflicts.iter_mut().filter(|c| !c.resolved) {
        let conf_a = beliefs.iter().find(|b| b.label == cf.belief_a)
            .map(|b| b.effective_confidence()).unwrap_or(0.0);
        let conf_b = beliefs.iter().find(|b| b.label == cf.belief_b)
            .map(|b| b.effective_confidence()).unwrap_or(0.0);
        let diff = (conf_a - conf_b).abs();
        if diff > 0.15 {
            cf.winner   = Some(if conf_a >= conf_b { cf.belief_a.clone() } else { cf.belief_b.clone() });
            cf.resolved = true;
            resolved   += 1;
            CONFLICTS_RESOLVED.fetch_add(1, Ordering::Relaxed);
        }
    }
    resolved
}

pub fn weak_evidence_conflicts() -> Vec<ProbabilisticConflict> {
    STATE.lock().unwrap().conflicts.iter()
        .filter(|c| !c.resolved && c.resolution_confidence < WEAK_THRESHOLD)
        .cloned()
        .collect()
}

pub fn active_conflicts() -> Vec<ProbabilisticConflict> {
    STATE.lock().unwrap().conflicts.iter().filter(|c| !c.resolved).cloned().collect()
}

pub fn conflict_count() -> usize { STATE.lock().unwrap().conflicts.len() }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_no_panic() {
        let _ = detect_conflicts();
    }

    #[test]
    fn resolve_no_panic() {
        let _ = resolve_by_confidence();
    }
}
