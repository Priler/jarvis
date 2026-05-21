//! Belief engine — probabilistic belief store with confidence, stability,
//! causal support, contradiction pressure, evidence strength, and decay rate.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static BELIEFS_ASSERTED: AtomicU64 = AtomicU64::new(0);
pub static BELIEFS_DECAYED:  AtomicU64 = AtomicU64::new(0);

pub const MIN_RELIABLE_CONFIDENCE: f32 = 0.50;
pub const DEFAULT_DECAY_RATE:      f32 = 0.005;
pub const MAX_BELIEFS:             usize = 300;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── EvidenceStrength / UncertaintyLevel ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EvidenceStrength { Strong, Medium, Weak, Absent }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UncertaintyLevel { Low, Moderate, High, Critical }

// ── Belief ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Belief {
    pub id:                     u64,
    pub label:                  String,
    pub confidence:             f32,    // 0–1
    pub stability:              f32,    // 0–1, stability of confidence over time
    pub causal_support:         f32,    // 0–1, strength of causal backing
    pub contradiction_pressure: f32,    // 0–1, how hard contradictions push against
    pub evidence_strength:      EvidenceStrength,
    pub decay_rate:             f32,    // confidence lost per tick
    pub ts_ms:                  u64,
}

impl Belief {
    pub fn uncertainty_level(&self) -> UncertaintyLevel {
        let u = 1.0 - self.confidence;
        if u < 0.25      { UncertaintyLevel::Low }
        else if u < 0.50 { UncertaintyLevel::Moderate }
        else if u < 0.75 { UncertaintyLevel::High }
        else             { UncertaintyLevel::Critical }
    }

    pub fn is_reliable(&self) -> bool {
        self.confidence >= MIN_RELIABLE_CONFIDENCE
            && self.evidence_strength != EvidenceStrength::Absent
            && self.contradiction_pressure < 0.60
    }

    pub fn apply_decay(&mut self) {
        self.confidence = (self.confidence - self.decay_rate).max(0.0);
    }

    // confidence minus contradiction penalty
    pub fn effective_confidence(&self) -> f32 {
        (self.confidence - self.contradiction_pressure * 0.30).max(0.0)
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct BeliefStore {
    beliefs: Vec<Belief>,
    seq:     u64,
}

static STORE: Lazy<Mutex<BeliefStore>> = Lazy::new(|| {
    Mutex::new(BeliefStore { beliefs: Vec::new(), seq: 0 })
});

// ── API ───────────────────────────────────────────────────────────────────────

pub fn assert_belief(
    label:         impl Into<String>,
    confidence:    f32,
    evidence:      EvidenceStrength,
    causal_support: f32,
) -> u64 {
    let label      = label.into();
    let confidence = confidence.clamp(0.0, 1.0);
    let mut s      = STORE.lock().unwrap();

    // Update existing belief
    if let Some(b) = s.beliefs.iter_mut().find(|b| b.label == label) {
        let old_conf = b.confidence;
        b.confidence     = (b.confidence * 0.70 + confidence * 0.30).clamp(0.0, 1.0);
        b.causal_support = (b.causal_support * 0.60 + causal_support.clamp(0.0, 1.0) * 0.40).clamp(0.0, 1.0);
        if confidence > old_conf { b.evidence_strength = evidence; }
        b.ts_ms = ts_now();
        BELIEFS_ASSERTED.fetch_add(1, Ordering::Relaxed);
        return b.id;
    }

    // New belief — extract seq before push to avoid E0502
    s.seq += 1;
    let id = s.seq;
    let belief = Belief {
        id,
        label,
        confidence,
        stability:              0.50,
        causal_support:         causal_support.clamp(0.0, 1.0),
        contradiction_pressure: 0.0,
        evidence_strength:      evidence,
        decay_rate:             DEFAULT_DECAY_RATE,
        ts_ms:                  ts_now(),
    };
    if s.beliefs.len() >= MAX_BELIEFS { s.beliefs.remove(0); }
    s.beliefs.push(belief);
    BELIEFS_ASSERTED.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn apply_contradiction_pressure(label: &str, pressure: f32) {
    let mut s = STORE.lock().unwrap();
    if let Some(b) = s.beliefs.iter_mut().find(|b| b.label == label) {
        b.contradiction_pressure = (b.contradiction_pressure + pressure).clamp(0.0, 1.0);
    }
}

pub fn reinforce(label: &str, delta: f32) {
    let mut s = STORE.lock().unwrap();
    if let Some(b) = s.beliefs.iter_mut().find(|b| b.label == label) {
        b.confidence = (b.confidence + delta).clamp(0.0, 1.0);
    }
}

pub fn decay_all() {
    let mut s = STORE.lock().unwrap();
    for b in &mut s.beliefs {
        b.apply_decay();
        b.stability = (b.stability * 0.95 + b.confidence * 0.05).clamp(0.0, 1.0);
    }
    BELIEFS_DECAYED.fetch_add(1, Ordering::Relaxed);
}

pub fn all_beliefs() -> Vec<Belief> {
    STORE.lock().unwrap().beliefs.clone()
}

pub fn reliable_beliefs() -> Vec<Belief> {
    STORE.lock().unwrap().beliefs.iter().filter(|b| b.is_reliable()).cloned().collect()
}

pub fn avg_confidence() -> f32 {
    let s = STORE.lock().unwrap();
    if s.beliefs.is_empty() { return 0.50; }
    s.beliefs.iter().map(|b| b.confidence).sum::<f32>() / s.beliefs.len() as f32
}

pub fn avg_uncertainty() -> f32 { 1.0 - avg_confidence() }

pub fn belief_count() -> usize { STORE.lock().unwrap().beliefs.len() }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_and_retrieve() {
        let id = assert_belief("test_be_ph22", 0.7, EvidenceStrength::Medium, 0.5);
        assert!(all_beliefs().iter().any(|b| b.id == id));
    }

    #[test]
    fn decay_no_panic() {
        assert_belief("decay_be_ph22", 0.9, EvidenceStrength::Strong, 0.7);
        decay_all();
    }

    #[test]
    fn uncertainty_level_moderate() {
        let b = Belief {
            id: 0, label: "x".into(), confidence: 0.3, stability: 0.5,
            causal_support: 0.4, contradiction_pressure: 0.0,
            evidence_strength: EvidenceStrength::Weak, decay_rate: 0.005, ts_ms: 0,
        };
        assert_eq!(b.uncertainty_level(), UncertaintyLevel::Moderate);
    }

    #[test]
    fn effective_confidence_applies_pressure() {
        let b = Belief {
            id: 0, label: "y".into(), confidence: 0.8, stability: 0.5,
            causal_support: 0.6, contradiction_pressure: 0.5,
            evidence_strength: EvidenceStrength::Medium, decay_rate: 0.005, ts_ms: 0,
        };
        assert!(b.effective_confidence() < b.confidence);
    }
}
