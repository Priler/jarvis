//! Strategy evaluator — scores planner, recovery, verification, workflow,
//! attention, and prediction quality from observed runtime counters.
//! Pure heuristic; no ML, no LLM, no network.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static EVALUATIONS_RUN:    AtomicU64 = AtomicU64::new(0);
pub static EVALUATIONS_LOW:    AtomicU64 = AtomicU64::new(0);

// ── Score ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyScore {
    pub dimension:   String,
    pub score:       f32,   // 0.0–1.0
    pub confidence:  f32,
    pub sample_size: u32,
    pub ts_ms:       u64,
}

impl StrategyScore {
    pub fn is_low(&self) -> bool { self.score < 0.5 }
    pub fn is_critical(&self) -> bool { self.score < 0.3 }
    pub fn grade(&self) -> &'static str {
        match (self.score * 10.0) as u32 {
            0..=2 => "F", 3..=4 => "D", 5..=6 => "C", 7..=8 => "B", _ => "A",
        }
    }
}

// ── Dimension keys ────────────────────────────────────────────────────────────

pub const DIM_PLANNER:      &str = "planner_quality";
pub const DIM_RECOVERY:     &str = "recovery_quality";
pub const DIM_VERIFICATION: &str = "verification_quality";
pub const DIM_WORKFLOW:     &str = "workflow_success_rate";
pub const DIM_ATTENTION:    &str = "attention_quality";
pub const DIM_PREDICTION:   &str = "prediction_quality";

// ── History ───────────────────────────────────────────────────────────────────

const MAX_SCORE_HISTORY: usize = 60;

static HISTORY: Lazy<Mutex<Vec<StrategyScore>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Public API ────────────────────────────────────────────────────────────────

/// Compute scores for all 6 dimensions and store them.
pub fn evaluate() -> Vec<StrategyScore> {
    EVALUATIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let scores = vec![
        score_planner(now),
        score_recovery(now),
        score_verification(now),
        score_workflow(now),
        score_attention(now),
        score_prediction(now),
    ];

    let low_count = scores.iter().filter(|s| s.is_low()).count();
    EVALUATIONS_LOW.fetch_add(low_count as u64, Ordering::Relaxed);

    if let Ok(mut h) = HISTORY.lock() {
        for s in &scores {
            if h.len() >= MAX_SCORE_HISTORY { h.remove(0); }
            h.push(s.clone());
        }
    }
    scores
}

pub fn latest(dimension: &str) -> Option<StrategyScore> {
    HISTORY.lock().ok().and_then(|h| {
        h.iter().filter(|s| s.dimension == dimension).last().cloned()
    })
}

pub fn all_latest() -> Vec<StrategyScore> {
    let dims = [DIM_PLANNER, DIM_RECOVERY, DIM_VERIFICATION,
                DIM_WORKFLOW, DIM_ATTENTION, DIM_PREDICTION];
    dims.iter().filter_map(|d| latest(d)).collect()
}

pub fn history_len() -> usize {
    HISTORY.lock().map(|h| h.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut h) = HISTORY.lock() { h.clear(); }
}

// ── Scoring heuristics ────────────────────────────────────────────────────────

fn score_planner(now: u64) -> StrategyScore {
    let ticks      = crate::cognitive_memory::count() as u32;
    let rate       = crate::cognitive_memory::recent_success_rate(20);
    let score      = (rate * 0.7 + (ticks.min(20) as f32 / 20.0) * 0.3).min(1.0);
    StrategyScore { dimension: DIM_PLANNER.into(), score, confidence: 0.7, sample_size: ticks, ts_ms: now }
}

fn score_recovery(now: u64) -> StrategyScore {
    let anomalies = crate::anomaly_detector::ANOMALY_CHECKS.load(Ordering::Relaxed) as u32;
    let found     = crate::anomaly_detector::ANOMALIES_FOUND.load(Ordering::Relaxed) as u32;
    let score = if anomalies == 0 { 0.5 } else {
        let rate = found as f32 / anomalies as f32;
        (1.0 - rate).max(0.0)
    };
    StrategyScore { dimension: DIM_RECOVERY.into(), score, confidence: 0.6, sample_size: anomalies, ts_ms: now }
}

fn score_verification(now: u64) -> StrategyScore {
    let ticks = crate::cognitive_memory::count() as u32;
    let rate  = crate::cognitive_memory::recent_success_rate(10);
    StrategyScore { dimension: DIM_VERIFICATION.into(), score: rate, confidence: 0.65, sample_size: ticks, ts_ms: now }
}

fn score_workflow(now: u64) -> StrategyScore {
    let recorded = crate::workflow_learning::SEQUENCES_RECORDED.load(Ordering::Relaxed) as u32;
    let patterns = crate::workflow_learning::PATTERNS_LEARNED.load(Ordering::Relaxed) as u32;
    let score = if recorded == 0 { 0.5 } else {
        (patterns as f32 / recorded.max(1) as f32 * 10.0).min(1.0)
    };
    StrategyScore { dimension: DIM_WORKFLOW.into(), score, confidence: 0.6, sample_size: recorded, ts_ms: now }
}

fn score_attention(now: u64) -> StrategyScore {
    let evals  = crate::attention_runtime::ATTENTION_EVALUATIONS.load(Ordering::Relaxed) as u32;
    let shifts = crate::attention_runtime::ATTENTION_SHIFTS.load(Ordering::Relaxed) as u32;
    let score = if evals == 0 { 0.5 } else {
        let churn = shifts as f32 / evals as f32;
        (1.0 - churn.min(1.0)).max(0.0) * 0.8 + 0.2
    };
    StrategyScore { dimension: DIM_ATTENTION.into(), score, confidence: 0.6, sample_size: evals, ts_ms: now }
}

fn score_prediction(now: u64) -> StrategyScore {
    let made     = crate::predictive_reasoner::PREDICTIONS_MADE.load(Ordering::Relaxed) as u32;
    let verified = crate::predictive_reasoner::PREDICTIONS_VERIFIED.load(Ordering::Relaxed) as u32;
    let correct  = crate::predictive_reasoner::PREDICTIONS_CORRECT.load(Ordering::Relaxed) as u32;
    let score = if verified == 0 { 0.5 } else {
        correct as f32 / verified as f32
    };
    StrategyScore { dimension: DIM_PREDICTION.into(), score, confidence: if verified > 5 { 0.8 } else { 0.4 }, sample_size: made, ts_ms: now }
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
    fn evaluate_returns_six_dimensions() {
        let scores = evaluate();
        assert_eq!(scores.len(), 6);
    }

    #[test]
    fn score_grade_ranges() {
        // 0.95*10=9.5→9 → "A"; 0.35*10=3.5→3 → "D"
        let s = StrategyScore { dimension: "x".into(), score: 0.95, confidence: 0.9, sample_size: 10, ts_ms: 0 };
        assert_eq!(s.grade(), "A");
        let s2 = StrategyScore { score: 0.35, ..s.clone() };
        assert_eq!(s2.grade(), "D");
    }

    #[test]
    fn score_is_low_threshold() {
        let s = StrategyScore { dimension: "x".into(), score: 0.4, confidence: 0.5, sample_size: 5, ts_ms: 0 };
        assert!(s.is_low());
        let s2 = StrategyScore { score: 0.7, ..s };
        assert!(!s2.is_low());
    }

    #[test]
    fn history_grows_after_evaluate() {
        let before = EVALUATIONS_RUN.load(Ordering::Relaxed);
        evaluate();
        assert!(EVALUATIONS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn scores_bounded_zero_to_one() {
        for s in evaluate() {
            assert!(s.score >= 0.0 && s.score <= 1.0, "score out of range: {}", s.score);
        }
    }

    #[test]
    fn all_latest_returns_dimensions() {
        evaluate();
        let latest = all_latest();
        assert!(!latest.is_empty());
    }
}
