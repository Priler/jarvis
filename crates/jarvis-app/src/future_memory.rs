//! Future memory — persists simulation outcomes, predicted futures, synthetic
//! cognition structures, instability patterns, and semantic evolution forecasts.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static ENTRIES_STORED: AtomicU64 = AtomicU64::new(0);

const MAX_ENTRIES: usize = 500;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── FutureCategory ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureCategory {
    SimulationOutcome,
    PredictedFuture,
    SyntheticCognition,
    InstabilityPattern,
    SemanticForecast,
}

impl FutureCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SimulationOutcome => "simulation_outcome",
            Self::PredictedFuture   => "predicted_future",
            Self::SyntheticCognition=> "synthetic_cognition",
            Self::InstabilityPattern=> "instability_pattern",
            Self::SemanticForecast  => "semantic_forecast",
        }
    }
}

// ── FutureMemoryEntry ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FutureMemoryEntry {
    pub id:          u64,
    pub category:    FutureCategory,
    pub content:     String,
    pub instability: f32,
    pub ts_ms:       u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct FutureStore {
    entries: Vec<FutureMemoryEntry>,
    seq:     u64,
}

impl FutureStore {
    fn new() -> Self { FutureStore { entries: Vec::new(), seq: 0 } }
}

static STORE: Lazy<Mutex<FutureStore>> = Lazy::new(|| Mutex::new(FutureStore::new()));

// ── API ───────────────────────────────────────────────────────────────────────

pub fn store(category: FutureCategory, content: impl Into<String>, instability: f32) -> u64 {
    let mut s = STORE.lock().unwrap();
    s.seq += 1;
    let id = s.seq;
    if s.entries.len() >= MAX_ENTRIES { s.entries.remove(0); }
    s.entries.push(FutureMemoryEntry {
        id,
        category,
        content: content.into(),
        instability: instability.clamp(0.0, 1.0),
        ts_ms: ts_now(),
    });
    ENTRIES_STORED.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn recent(n: usize) -> Vec<FutureMemoryEntry> {
    STORE.lock().unwrap().entries.iter().rev().take(n).cloned().collect()
}

pub fn by_category(cat: FutureCategory) -> Vec<FutureMemoryEntry> {
    STORE.lock().unwrap().entries.iter()
        .filter(|e| e.category == cat)
        .cloned()
        .collect()
}

pub fn count() -> usize {
    STORE.lock().unwrap().entries.len()
}

pub fn avg_instability() -> f32 {
    let s = STORE.lock().unwrap();
    if s.entries.is_empty() { return 0.0; }
    s.entries.iter().map(|e| e.instability).sum::<f32>() / s.entries.len() as f32
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_retrieve() {
        let id = store(FutureCategory::PredictedFuture, "test_prediction", 0.3);
        assert!(id > 0);
        assert!(count() > 0);
    }

    #[test]
    fn by_category_filters() {
        store(FutureCategory::SimulationOutcome, "outcome_a", 0.5);
        let outcomes = by_category(FutureCategory::SimulationOutcome);
        assert!(!outcomes.is_empty());
    }

    #[test]
    fn avg_instability_bounded() {
        store(FutureCategory::InstabilityPattern, "instability_x", 0.7);
        let avg = avg_instability();
        assert!(avg >= 0.0 && avg <= 1.0);
    }
}
