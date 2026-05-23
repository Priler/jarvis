//! Failure pattern analyzer — detects repeated failures, planner weaknesses,
//! workflow instability, and adaptation regressions.
//! No ML. Frequency counting over a bounded window.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static ANALYSES_RUN:      AtomicU64 = AtomicU64::new(0);
pub static PATTERNS_DETECTED: AtomicU64 = AtomicU64::new(0);

const MAX_FAILURE_LOG:  usize = 200;
const REPEAT_THRESHOLD: u32   = 3;

// ── Failure kinds ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FailureKind {
    PlannerFailure { reason: String },
    WorkflowStall  { tool_id: String },
    AnomalyRepeat  { label: String },
    VerifyFail     { tool_id: String },
    AttentionLoop  { focus: String },
    RecoveryFail   { context: String },
}

impl FailureKind {
    pub fn key(&self) -> String {
        match self {
            FailureKind::PlannerFailure { reason }  => format!("planner:{reason}"),
            FailureKind::WorkflowStall  { tool_id } => format!("stall:{tool_id}"),
            FailureKind::AnomalyRepeat  { label }   => format!("anomaly:{label}"),
            FailureKind::VerifyFail     { tool_id } => format!("verify:{tool_id}"),
            FailureKind::AttentionLoop  { focus }   => format!("attn:{focus}"),
            FailureKind::RecoveryFail   { context } => format!("recovery:{context}"),
        }
    }
}

// ── Detected pattern ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FailurePattern {
    pub key:        String,
    pub count:      u32,
    pub severity:   &'static str,   // "critical" | "high" | "medium"
    pub ts_ms:      u64,
}

impl FailurePattern {
    pub fn is_critical(&self) -> bool { self.severity == "critical" }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct AnalyzerState {
    log:    Vec<FailureKind>,
    counts: HashMap<String, u32>,
}

static STATE: Lazy<Mutex<AnalyzerState>> = Lazy::new(|| Mutex::new(AnalyzerState {
    log:    Vec::new(),
    counts: HashMap::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn record(kind: FailureKind) {
    if let Ok(mut s) = STATE.lock() {
        if s.log.len() >= MAX_FAILURE_LOG { s.log.remove(0); }
        let key = kind.key();
        *s.counts.entry(key).or_insert(0) += 1;
        s.log.push(kind);
    }
}

pub fn analyze() -> Vec<FailurePattern> {
    ANALYSES_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let counts = STATE.lock().map(|s| s.counts.clone()).unwrap_or_default();

    let mut patterns: Vec<FailurePattern> = counts.into_iter()
        .filter(|(_, c)| *c >= REPEAT_THRESHOLD)
        .map(|(key, count)| {
            let severity = if count >= 10 { "critical" } else if count >= 5 { "high" } else { "medium" };
            FailurePattern { key, count, severity, ts_ms: now }
        })
        .collect();

    patterns.sort_by(|a, b| b.count.cmp(&a.count));
    PATTERNS_DETECTED.fetch_add(patterns.len() as u64, Ordering::Relaxed);
    patterns
}

pub fn top_failures(n: usize) -> Vec<FailurePattern> {
    let mut p = analyze();
    p.truncate(n);
    p
}

pub fn has_critical_pattern() -> bool {
    analyze().iter().any(|p| p.is_critical())
}

pub fn failure_count(key: &str) -> u32 {
    STATE.lock().map(|s| s.counts.get(key).copied().unwrap_or(0)).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut s) = STATE.lock() {
        s.log.clear();
        s.counts.clear();
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

    fn local_clear() {
        if let Ok(mut s) = STATE.lock() { s.log.clear(); s.counts.clear(); }
    }

    #[test]
    fn record_and_analyze_detects_repeat() {
        local_clear();
        let kind = FailureKind::PlannerFailure { reason: "fp.test.unique1".into() };
        for _ in 0..REPEAT_THRESHOLD {
            record(kind.clone());
        }
        let patterns = analyze();
        assert!(patterns.iter().any(|p| p.key.contains("fp.test.unique1")));
    }

    #[test]
    fn below_threshold_not_reported() {
        local_clear();
        let kind = FailureKind::WorkflowStall { tool_id: "fp.below.unique2".into() };
        for _ in 0..(REPEAT_THRESHOLD - 1) {
            record(kind.clone());
        }
        let patterns = analyze();
        assert!(!patterns.iter().any(|p| p.key.contains("fp.below.unique2")));
    }

    #[test]
    fn critical_severity_at_ten() {
        local_clear();
        let kind = FailureKind::AnomalyRepeat { label: "fp.crit.unique3".into() };
        for _ in 0..10 { record(kind.clone()); }
        let patterns = analyze();
        let p = patterns.iter().find(|p| p.key.contains("fp.crit.unique3")).unwrap();
        assert!(p.is_critical());
    }

    #[test]
    fn failure_count_tracks() {
        local_clear();
        let kind = FailureKind::VerifyFail { tool_id: "fp.count.unique4".into() };
        record(kind.clone());
        record(kind);
        assert!(failure_count("verify:fp.count.unique4") >= 2);
    }

    #[test]
    fn analyses_run_increments() {
        let before = ANALYSES_RUN.load(Ordering::Relaxed);
        analyze();
        assert!(ANALYSES_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn top_failures_truncates() {
        local_clear();
        for i in 0..5u32 {
            let kind = FailureKind::RecoveryFail { context: format!("fp.top.{i}") };
            for _ in 0..REPEAT_THRESHOLD { record(kind.clone()); }
        }
        let top = top_failures(2);
        assert!(top.len() <= 2);
    }
}
