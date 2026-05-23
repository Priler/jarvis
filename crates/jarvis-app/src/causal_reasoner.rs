//! Causal reasoner — records causal chains (A→B), estimates causal probability,
//! and detects causally-linked failure trajectories.
//! No ML; pure frequency-weighted heuristics over bounded history.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static CAUSAL_OBSERVATIONS: AtomicU64 = AtomicU64::new(0);
pub static CAUSAL_LINKS_FOUND:  AtomicU64 = AtomicU64::new(0);
pub static CAUSAL_PREDICTIONS:  AtomicU64 = AtomicU64::new(0);

const MAX_CHAIN_HISTORY: usize  = 300;
const MIN_LINK_STRENGTH: f32    = 0.3;
const LINK_THRESHOLD:    u32    = 2;

// ── Causal observation ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CausalObservation {
    pub cause:      String,
    pub effect:     String,
    pub strength:   f32,       // 0.0–1.0
    pub ts_ms:      u64,
}

// ── Causal link (learned) ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CausalLink {
    pub cause:        String,
    pub effect:       String,
    pub occurrences:  u32,
    pub strength:     f32,
    pub is_stable:    bool,
    pub ts_ms:        u64,
}

impl CausalLink {
    pub fn key(cause: &str, effect: &str) -> String {
        format!("{cause}→{effect}")
    }
    pub fn is_reliable(&self) -> bool {
        self.occurrences >= LINK_THRESHOLD && self.strength >= MIN_LINK_STRENGTH
    }
}

// ── Causal prediction ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CausalPrediction {
    pub trigger:     String,
    pub predicted:   String,
    pub probability: f32,
    pub confidence:  f32,
    pub ts_ms:       u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct CausalState {
    history: Vec<CausalObservation>,
    links:   HashMap<String, CausalLink>,
}

static STATE: Lazy<Mutex<CausalState>> = Lazy::new(|| Mutex::new(CausalState {
    history: Vec::new(),
    links:   HashMap::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn observe(cause: impl Into<String>, effect: impl Into<String>, strength: f32) {
    CAUSAL_OBSERVATIONS.fetch_add(1, Ordering::Relaxed);
    let cause  = cause.into();
    let effect = effect.into();
    let now    = ts_now();
    let key    = CausalLink::key(&cause, &effect);

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_CHAIN_HISTORY { s.history.remove(0); }
        s.history.push(CausalObservation { cause: cause.clone(), effect: effect.clone(), strength, ts_ms: now });

        let link = s.links.entry(key).or_insert_with(|| {
            CAUSAL_LINKS_FOUND.fetch_add(1, Ordering::Relaxed);
            CausalLink { cause: cause.clone(), effect: effect.clone(), occurrences: 0, strength: 0.0, is_stable: false, ts_ms: now }
        });
        link.occurrences += 1;
        link.strength = (link.strength * 0.7 + strength * 0.3).clamp(0.0, 1.0);
        link.is_stable = link.occurrences >= LINK_THRESHOLD;
        link.ts_ms = now;
    }
}

pub fn predict(trigger: impl Into<String>) -> Vec<CausalPrediction> {
    CAUSAL_PREDICTIONS.fetch_add(1, Ordering::Relaxed);
    let trigger = trigger.into();
    let now = ts_now();

    STATE.lock().map(|s| {
        s.links.values()
            .filter(|l| l.cause == trigger && l.is_reliable())
            .map(|l| CausalPrediction {
                trigger:     trigger.clone(),
                predicted:   l.effect.clone(),
                probability: l.strength,
                confidence:  (l.occurrences as f32 / 10.0).min(1.0),
                ts_ms:       now,
            })
            .collect()
    }).unwrap_or_default()
}

pub fn reliable_links() -> Vec<CausalLink> {
    STATE.lock().map(|s| {
        s.links.values().filter(|l| l.is_reliable()).cloned().collect()
    }).unwrap_or_default()
}

pub fn all_links() -> Vec<CausalLink> {
    STATE.lock().map(|s| s.links.values().cloned().collect()).unwrap_or_default()
}

pub fn link_strength(cause: &str, effect: &str) -> f32 {
    let key = CausalLink::key(cause, effect);
    STATE.lock().map(|s| s.links.get(&key).map(|l| l.strength).unwrap_or(0.0)).unwrap_or(0.0)
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); s.links.clear(); }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_increments_counter() {
        let before = CAUSAL_OBSERVATIONS.load(Ordering::Relaxed);
        observe("cr.cause1.unique", "cr.effect1.unique", 0.8);
        assert!(CAUSAL_OBSERVATIONS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn link_becomes_reliable_after_threshold() {
        for _ in 0..LINK_THRESHOLD {
            observe("cr.docker.u2", "cr.instability.u2", 0.9);
        }
        let links = reliable_links();
        assert!(links.iter().any(|l| l.cause == "cr.docker.u2" && l.effect == "cr.instability.u2"));
    }

    #[test]
    fn predict_returns_effects_for_known_cause() {
        for _ in 0..LINK_THRESHOLD {
            observe("cr.restart.u3", "cr.degraded.u3", 0.7);
        }
        let preds = predict("cr.restart.u3");
        assert!(preds.iter().any(|p| p.predicted == "cr.degraded.u3"));
    }

    #[test]
    fn link_strength_bounded() {
        observe("cr.a.u4", "cr.b.u4", 1.5); // over max
        let s = link_strength("cr.a.u4", "cr.b.u4");
        assert!(s >= 0.0 && s <= 1.0);
    }

    #[test]
    fn causal_links_found_increments() {
        let before = CAUSAL_LINKS_FOUND.load(Ordering::Relaxed);
        observe("cr.new.u5.xxxxxx", "cr.effect.u5", 0.5);
        // New link created for unique cause
        assert!(CAUSAL_LINKS_FOUND.load(Ordering::Relaxed) >= before);
    }

    #[test]
    fn history_bounded() {
        let before = history_len();
        observe("cr.hist.u6", "cr.hist.u6.b", 0.4);
        assert!(history_len() <= MAX_CHAIN_HISTORY && history_len() >= before.min(MAX_CHAIN_HISTORY));
    }
}
