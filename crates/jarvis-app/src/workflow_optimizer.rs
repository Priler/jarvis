//! Workflow optimizer — reduces repeated failures, optimizes startup sequences,
//! and improves execution stability by learning from workflow outcomes.
//! No ML; frequency-based heuristics over bounded history.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static OPTIMIZATIONS_APPLIED: AtomicU64 = AtomicU64::new(0);
pub static WORKFLOWS_IMPROVED:    AtomicU64 = AtomicU64::new(0);

const MAX_OUTCOME_HISTORY: usize = 200;
const STABLE_THRESHOLD:    u32   = 5;
const UNSTABLE_THRESHOLD:  u32   = 3;

// ── Workflow outcome ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OutcomeKind { Success, Failure { reason: String }, Timeout, Cancelled }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowOutcome {
    pub workflow_id: String,
    pub tool_id:     String,
    pub outcome:     OutcomeKind,
    pub latency_ms:  u64,
    pub ts_ms:       u64,
}

// ── Optimization suggestion ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OptimizationKind {
    SkipUnstableStep  { tool_id: String },
    PreferStableOrder { preferred: Vec<String> },
    AddRetry          { tool_id: String, max_retries: u32 },
    ReduceVerification{ tool_id: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OptimizationSuggestion {
    pub kind:       OptimizationKind,
    pub confidence: f32,
    pub ts_ms:      u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct OptimizerState {
    history:       Vec<WorkflowOutcome>,
    success_counts: HashMap<String, u32>,
    failure_counts: HashMap<String, u32>,
}

static STATE: Lazy<Mutex<OptimizerState>> = Lazy::new(|| Mutex::new(OptimizerState {
    history:        Vec::new(),
    success_counts: HashMap::new(),
    failure_counts: HashMap::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn record_outcome(outcome: WorkflowOutcome) {
    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_OUTCOME_HISTORY { s.history.remove(0); }
        match &outcome.outcome {
            OutcomeKind::Success => {
                *s.success_counts.entry(outcome.tool_id.clone()).or_insert(0) += 1;
            }
            OutcomeKind::Failure { .. } | OutcomeKind::Timeout => {
                *s.failure_counts.entry(outcome.tool_id.clone()).or_insert(0) += 1;
            }
            _ => {}
        }
        s.history.push(outcome);
    }
}

pub fn optimize() -> Vec<OptimizationSuggestion> {
    OPTIMIZATIONS_APPLIED.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let (success_counts, failure_counts) = {
        let s = STATE.lock().unwrap_or_else(|e| e.into_inner());
        (s.success_counts.clone(), s.failure_counts.clone())
    };

    let mut suggestions = Vec::new();

    // Unstable tools → suggest retry
    for (tool_id, fail_count) in &failure_counts {
        if *fail_count >= UNSTABLE_THRESHOLD {
            let success = success_counts.get(tool_id).copied().unwrap_or(0);
            let confidence = (*fail_count as f32 / (*fail_count + success + 1) as f32).min(1.0);
            suggestions.push(OptimizationSuggestion {
                kind:       OptimizationKind::AddRetry { tool_id: tool_id.clone(), max_retries: 2 },
                confidence,
                ts_ms:      now,
            });
            WORKFLOWS_IMPROVED.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Very unstable → suggest skipping
    for (tool_id, fail_count) in &failure_counts {
        let success = success_counts.get(tool_id).copied().unwrap_or(0);
        if *fail_count >= STABLE_THRESHOLD && success == 0 {
            suggestions.push(OptimizationSuggestion {
                kind:       OptimizationKind::SkipUnstableStep { tool_id: tool_id.clone() },
                confidence: 0.8,
                ts_ms:      now,
            });
        }
    }

    suggestions
}

pub fn success_rate(tool_id: &str) -> f32 {
    let s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let ok  = s.success_counts.get(tool_id).copied().unwrap_or(0) as f32;
    let bad = s.failure_counts.get(tool_id).copied().unwrap_or(0) as f32;
    if ok + bad == 0.0 { 0.5 } else { ok / (ok + bad) }
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() {
        s.history.clear();
        s.success_counts.clear();
        s.failure_counts.clear();
    }
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

    fn outcome(tool: &str, ok: bool) -> WorkflowOutcome {
        WorkflowOutcome {
            workflow_id: "wf1".into(), tool_id: tool.into(),
            outcome: if ok { OutcomeKind::Success } else { OutcomeKind::Failure { reason: "err".into() } },
            latency_ms: 100, ts_ms: 0,
        }
    }

    #[test]
    fn record_and_optimize_runs() {
        clear();
        record_outcome(outcome("wo.tool1", true));
        let _ = optimize();
    }

    #[test]
    fn unstable_tool_generates_retry_suggestion() {
        clear();
        let tool = "wo.unstable.unique1";
        for _ in 0..UNSTABLE_THRESHOLD {
            record_outcome(outcome(tool, false));
        }
        let suggestions = optimize();
        assert!(suggestions.iter().any(|s| matches!(&s.kind, OptimizationKind::AddRetry { tool_id, .. } if tool_id == tool)));
    }

    #[test]
    fn stable_tool_no_skip_suggestion() {
        clear();
        let tool = "wo.stable.unique2";
        for _ in 0..5 { record_outcome(outcome(tool, true)); }
        let suggestions = optimize();
        assert!(!suggestions.iter().any(|s| matches!(&s.kind, OptimizationKind::SkipUnstableStep { tool_id, .. } if tool_id == tool)));
    }

    #[test]
    fn success_rate_all_success() {
        clear();
        let tool = "wo.rate.unique3";
        for _ in 0..5 { record_outcome(outcome(tool, true)); }
        assert!(success_rate(tool) > 0.9);
    }

    #[test]
    fn optimizations_applied_increments() {
        let before = OPTIMIZATIONS_APPLIED.load(Ordering::Relaxed);
        optimize();
        assert!(OPTIMIZATIONS_APPLIED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn suggestions_confidence_bounded() {
        clear();
        for _ in 0..UNSTABLE_THRESHOLD {
            record_outcome(outcome("wo.conf.unique4", false));
        }
        for s in optimize() {
            assert!(s.confidence >= 0.0 && s.confidence <= 1.0);
        }
    }
}
