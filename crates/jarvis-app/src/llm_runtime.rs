//! High-level LLM runtime wrapper.
//!
//! Provides a single call site for inference with:
//!   - Observability counters (calls, failures, total latency)
//!   - Session-level prompt building via `LlmSession`
//!   - Automatic routing to the configured backend via `ModelRouter`
//!
//! Concrete backends (StubRuntime, OllamaRuntime, LlamaCppRuntime) are
//! defined in `llm_provider.rs` and wired in at startup via `init()`.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;

use crate::cognitive::model_runtime::{
    InferenceRequest, InferenceResponse, ModelRouter,
};
use crate::llm_config::{LlmBackend, LlmConfig};
use crate::llm_provider::{LlamaCppRuntime, OllamaRuntime, StubRuntime};
use crate::llm_session::LlmSession;

// ── Observability counters ────────────────────────────────────────────────────

pub static LLM_CALLS:            AtomicU64 = AtomicU64::new(0);
pub static LLM_FAILURES:         AtomicU64 = AtomicU64::new(0);
pub static LLM_LATENCY_TOTAL_MS: AtomicU64 = AtomicU64::new(0);

// ── Global router (set once at init) ─────────────────────────────────────────

static ROUTER: OnceCell<Mutex<ModelRouter>> = OnceCell::new();

/// Initialize the global LLM router from config.
///
/// Call once at startup. Subsequent calls are no-ops.
pub fn init(cfg: &LlmConfig) {
    ROUTER.get_or_init(|| {
        let router = build_router(cfg);
        Mutex::new(router)
    });
}

fn build_router(cfg: &LlmConfig) -> ModelRouter {
    match cfg.backend {
        LlmBackend::Stub => ModelRouter::new_null()
            .with_tier1(Box::new(StubRuntime)),
        LlmBackend::Ollama => ModelRouter::new_null()
            .with_tier1(Box::new(OllamaRuntime::new(cfg))),
        LlmBackend::LlamaCpp => ModelRouter::new_null()
            .with_tier1(Box::new(LlamaCppRuntime::new(cfg))),
    }
}

fn router() -> &'static Mutex<ModelRouter> {
    ROUTER.get_or_init(|| Mutex::new(ModelRouter::new_null()))
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Run an enrichment inference (Tier 1 — latency budget ≤ 200 ms).
pub fn enrich(prompt: impl Into<String>) -> Result<InferenceResponse, String> {
    run(InferenceRequest::enrichment(prompt))
}

/// Run a planning inference (Tier 2 — latency budget ≤ 2000 ms).
pub fn plan(prompt: impl Into<String>) -> Result<InferenceResponse, String> {
    run(InferenceRequest::planning(prompt))
}

/// Run a knowledge query (Tier 3 — no real-time constraint).
pub fn query(prompt: impl Into<String>) -> Result<InferenceResponse, String> {
    run(InferenceRequest::knowledge(prompt))
}

/// Run inference from a multi-turn session context.
///
/// Builds the flat prompt from the session and issues a `Planning` request.
pub fn infer_session(session: &LlmSession) -> Result<InferenceResponse, String> {
    let prompt = session.build_prompt();
    run(InferenceRequest::planning(prompt))
}

/// True if any backend is currently loaded.
pub fn is_ready() -> bool {
    router().lock().is_any_loaded()
}

/// Snapshot of current counters for observability.
pub fn counters() -> LlmCounters {
    LlmCounters {
        calls:            LLM_CALLS.load(Ordering::Relaxed),
        failures:         LLM_FAILURES.load(Ordering::Relaxed),
        latency_total_ms: LLM_LATENCY_TOTAL_MS.load(Ordering::Relaxed),
    }
}

// ── Internal ──────────────────────────────────────────────────────────────────

fn run(req: InferenceRequest) -> Result<InferenceResponse, String> {
    LLM_CALLS.fetch_add(1, Ordering::Relaxed);
    let result = router().lock().route(req);
    match &result {
        Ok(resp) => {
            LLM_LATENCY_TOTAL_MS.fetch_add(resp.latency_ms, Ordering::Relaxed);
        }
        Err(_) => {
            LLM_FAILURES.fetch_add(1, Ordering::Relaxed);
        }
    }
    result
}

// ── Counter snapshot ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct LlmCounters {
    pub calls:            u64,
    pub failures:         u64,
    pub latency_total_ms: u64,
}

impl LlmCounters {
    pub fn avg_latency_ms(&self) -> u64 {
        if self.calls == 0 { 0 } else { self.latency_total_ms / self.calls }
    }
    pub fn failure_rate(&self) -> f64 {
        if self.calls == 0 { 0.0 } else { self.failures as f64 / self.calls as f64 }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_with_stub_backend_succeeds() {
        init(&LlmConfig::default());
        let resp = enrich("open calculator").unwrap();
        assert!(!resp.text.is_empty());
    }

    #[test]
    fn plan_with_stub_backend_succeeds() {
        init(&LlmConfig::default());
        let resp = plan("multi-step task").unwrap();
        assert!(!resp.text.is_empty());
    }

    #[test]
    fn counters_increment_on_calls() {
        init(&LlmConfig::default());
        let before = LLM_CALLS.load(Ordering::Relaxed);
        enrich("test").ok();
        assert!(LLM_CALLS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn avg_latency_zero_when_no_calls() {
        let c = LlmCounters { calls: 0, failures: 0, latency_total_ms: 0 };
        assert_eq!(c.avg_latency_ms(), 0);
    }

    #[test]
    fn failure_rate_is_zero_when_no_calls() {
        let c = LlmCounters { calls: 0, failures: 0, latency_total_ms: 0 };
        assert!((c.failure_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn infer_session_uses_session_prompt() {
        init(&LlmConfig::default());
        let mut session = LlmSession::new("You are Jarvis.");
        session.push_user("open calculator");
        let resp = infer_session(&session).unwrap();
        assert!(!resp.text.is_empty());
    }
}
