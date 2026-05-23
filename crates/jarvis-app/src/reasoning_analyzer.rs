//! Reasoning analyzer — scores the quality of reasoning steps across 5 dimensions
//! using runtime counters.  No ML; pure heuristic scoring.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static ANALYSES_RUN:         AtomicU64 = AtomicU64::new(0);
pub static LOW_QUALITY_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static REASONING_IMPROVED:   AtomicU64 = AtomicU64::new(0);

const MAX_ANALYSIS_HISTORY: usize = 80;

// ── Reasoning quality report ──────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReasoningQuality {
    pub coherence:     f32,   // steps follow logically
    pub completeness:  f32,   // all required steps present
    pub consistency:   f32,   // no contradictions
    pub efficiency:    f32,   // minimal steps to goal
    pub robustness:    f32,   // handles edge cases
    pub overall:       f32,
    pub ts_ms:         u64,
}

impl ReasoningQuality {
    pub fn is_low(&self) -> bool { self.overall < 0.5 }
    pub fn is_critical(&self) -> bool { self.overall < 0.3 }
    pub fn grade(&self) -> &'static str {
        match (self.overall * 10.0) as u32 {
            0..=2 => "F",
            3..=4 => "D",
            5..=6 => "C",
            7..=8 => "B",
            _     => "A",
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct AnalyzerState {
    history: Vec<ReasoningQuality>,
}

static STATE: Lazy<Mutex<AnalyzerState>> = Lazy::new(|| Mutex::new(AnalyzerState {
    history: Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn analyze() -> ReasoningQuality {
    ANALYSES_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let coherence    = score_coherence();
    let completeness = score_completeness();
    let consistency  = score_consistency();
    let efficiency   = score_efficiency();
    let robustness   = score_robustness();

    let overall = (coherence * 0.25 + completeness * 0.20 + consistency * 0.25
        + efficiency * 0.15 + robustness * 0.15).clamp(0.0, 1.0);

    let q = ReasoningQuality { coherence, completeness, consistency, efficiency, robustness, overall, ts_ms: now };

    if q.is_low() { LOW_QUALITY_DETECTED.fetch_add(1, Ordering::Relaxed); }

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_ANALYSIS_HISTORY { s.history.remove(0); }
        let prev = s.history.last().map(|p| p.overall).unwrap_or(q.overall);
        if q.overall > prev { REASONING_IMPROVED.fetch_add(1, Ordering::Relaxed); }
        s.history.push(q.clone());
    }

    q
}

pub fn latest() -> Option<ReasoningQuality> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn average_overall(window: usize) -> f32 {
    STATE.lock().map(|s| {
        let slice: Vec<f32> = s.history.iter().rev().take(window).map(|q| q.overall).collect();
        if slice.is_empty() { return 0.5; }
        slice.iter().sum::<f32>() / slice.len() as f32
    }).unwrap_or(0.5)
}

pub fn history_len() -> usize {
    STATE.lock().map(|s| s.history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() { s.history.clear(); }
}

fn score_coherence() -> f32 {
    let obs = crate::causal_reasoner::CAUSAL_OBSERVATIONS.load(Ordering::Relaxed);
    let links = crate::causal_reasoner::reliable_links().len() as f32;
    if obs == 0 { return 0.6; }
    (links / (obs as f32).max(1.0) * 5.0).clamp(0.4, 0.95)
}

fn score_completeness() -> f32 {
    let q = crate::execution_quality::latest().map(|s| s.overall).unwrap_or(0.5);
    (q * 0.8 + 0.15).clamp(0.0, 1.0)
}

fn score_consistency() -> f32 {
    let drift = crate::cognitive_drift_control::recent_events(5).len();
    (1.0 - (drift as f32 / 5.0)).clamp(0.3, 1.0)
}

fn score_efficiency() -> f32 {
    let gen = crate::cognitive_evolution::generation() as f32;
    (0.5 + (gen / 100.0).min(0.4)).clamp(0.0, 1.0)
}

fn score_robustness() -> f32 {
    let patterns = crate::failure_pattern_analyzer::analyze().len();
    (1.0 - (patterns as f32 / 10.0).min(0.6)).clamp(0.3, 1.0)
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
    fn analyze_returns_bounded_scores() {
        let q = analyze();
        assert!(q.overall >= 0.0 && q.overall <= 1.0);
        assert!(q.coherence >= 0.0 && q.coherence <= 1.0);
    }

    #[test]
    fn analyses_run_counter_increments() {
        let before = ANALYSES_RUN.load(Ordering::Relaxed);
        analyze();
        assert!(ANALYSES_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn grade_returns_valid_letter() {
        let q = analyze();
        let g = q.grade();
        assert!(["A", "B", "C", "D", "F"].contains(&g));
    }

    #[test]
    fn average_overall_bounded() {
        analyze();
        let avg = average_overall(5);
        assert!(avg >= 0.0 && avg <= 1.0);
    }

    #[test]
    fn latest_returns_most_recent() {
        analyze();
        assert!(latest().is_some());
    }

    #[test]
    fn is_low_threshold() {
        let q = ReasoningQuality {
            coherence: 0.2, completeness: 0.2, consistency: 0.2,
            efficiency: 0.2, robustness: 0.2, overall: 0.2, ts_ms: 0,
        };
        assert!(q.is_low());
    }
}
