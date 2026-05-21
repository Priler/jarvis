//! Observability for the semantic intent runtime.
//!
//! Aggregates counters from llm_runtime, semantic_intent, tool_executor,
//! and ai_safety_runtime into a single `SemanticSnapshot` that is
//! periodically written to `semantic_snapshot.json`.
//!
//! No side-effects beyond file I/O.

use std::sync::atomic::Ordering;
use std::time::SystemTime;

// ── Counters (exported for cross-module access) ───────────────────────────────

// Re-export the per-module counters for unified reads.
pub use crate::llm_runtime::{LLM_CALLS, LLM_FAILURES, LLM_LATENCY_TOTAL_MS};
pub use crate::semantic_intent::{FALLBACK_PARSES, INTENT_PARSES, LLM_ENRICHMENTS};
pub use crate::tool_executor::{TOOL_BLOCKED, TOOL_CALLS, TOOL_FAILURES, TOOL_SUCCESSES, TOOL_TIMEOUTS};
pub use crate::ai_safety_runtime::{SAFETY_BLOCKED, SAFETY_CHECKS, SAFETY_PASSED};

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct SemanticSnapshot {
    pub ts_ms:              u64,
    pub llm_calls:          u64,
    pub llm_failures:       u64,
    pub llm_avg_latency_ms: u64,
    pub intent_parses:      u64,
    pub llm_enrichments:    u64,
    pub fallback_parses:    u64,
    pub tool_calls:         u64,
    pub tool_successes:     u64,
    pub tool_failures:      u64,
    pub tool_blocked:       u64,
    pub tool_timeouts:      u64,
    pub safety_checks:      u64,
    pub safety_blocked:     u64,
    pub safety_passed:      u64,
}

impl SemanticSnapshot {
    pub fn collect() -> Self {
        let llm_calls = LLM_CALLS.load(Ordering::Relaxed);
        let llm_latency = LLM_LATENCY_TOTAL_MS.load(Ordering::Relaxed);
        let llm_avg_latency_ms = if llm_calls > 0 { llm_latency / llm_calls } else { 0 };

        Self {
            ts_ms:              now_ms(),
            llm_calls,
            llm_failures:       LLM_FAILURES.load(Ordering::Relaxed),
            llm_avg_latency_ms,
            intent_parses:      INTENT_PARSES.load(Ordering::Relaxed),
            llm_enrichments:    LLM_ENRICHMENTS.load(Ordering::Relaxed),
            fallback_parses:    FALLBACK_PARSES.load(Ordering::Relaxed),
            tool_calls:         TOOL_CALLS.load(Ordering::Relaxed),
            tool_successes:     TOOL_SUCCESSES.load(Ordering::Relaxed),
            tool_failures:      TOOL_FAILURES.load(Ordering::Relaxed),
            tool_blocked:       TOOL_BLOCKED.load(Ordering::Relaxed),
            tool_timeouts:      TOOL_TIMEOUTS.load(Ordering::Relaxed),
            safety_checks:      SAFETY_CHECKS.load(Ordering::Relaxed),
            safety_blocked:     SAFETY_BLOCKED.load(Ordering::Relaxed),
            safety_passed:      SAFETY_PASSED.load(Ordering::Relaxed),
        }
    }

    /// Human-readable summary line for logs.
    pub fn summary(&self) -> String {
        format!(
            "llm={}/{} intents={} tools={}/{} safety_blocked={}",
            self.llm_calls - self.llm_failures, self.llm_calls,
            self.intent_parses,
            self.tool_successes, self.tool_calls,
            self.safety_blocked,
        )
    }
}

/// Write the current snapshot to `semantic_snapshot.json` (overwrite).
pub fn write_snapshot() {
    let snap = SemanticSnapshot::collect();
    if let Ok(json) = serde_json::to_string_pretty(&snap) {
        let _ = std::fs::write("semantic_snapshot.json", json);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_collects_without_panic() {
        let snap = SemanticSnapshot::collect();
        // ts_ms must be non-zero (we're past epoch 0).
        assert!(snap.ts_ms > 0);
    }

    #[test]
    fn snapshot_summary_is_not_empty() {
        let snap = SemanticSnapshot::collect();
        assert!(!snap.summary().is_empty());
    }

    #[test]
    fn snapshot_summary_contains_llm_field() {
        let snap = SemanticSnapshot::collect();
        assert!(snap.summary().contains("llm="));
    }

    #[test]
    fn avg_latency_is_zero_when_no_calls() {
        // This tests the formula directly, not the global state.
        let llm_calls: u64 = 0;
        let llm_latency: u64 = 1000;
        let avg = if llm_calls > 0 { llm_latency / llm_calls } else { 0 };
        assert_eq!(avg, 0);
    }
}
