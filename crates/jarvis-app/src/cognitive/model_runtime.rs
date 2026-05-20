#![allow(dead_code)]

//! Local model runtime abstraction.
//!
//! Defines the `ModelRuntime` trait that all local inference backends implement.
//! The `NullRuntime` is the default when no model is loaded.
//! The `ModelRouter` selects which tier to use for a given request kind.

use std::time::SystemTime;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Request/response types ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceKind {
    IntentEnrichment,   // Tier 1: short, latency-critical (budget ≤ 200 ms)
    Planning,           // Tier 2: multi-step reasoning (budget ≤ 2000 ms)
    KnowledgeQuery,     // Tier 3: long-form, no real-time constraint
    EmbeddingOnly,      // No generation — only embedding vector needed
}

#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub kind: InferenceKind,
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,   // 0.0 = deterministic, 0.7 = default
    pub timeout_ms: u64,
}

impl InferenceRequest {
    pub fn enrichment(prompt: impl Into<String>) -> Self {
        Self {
            kind: InferenceKind::IntentEnrichment,
            prompt: prompt.into(),
            max_tokens: 128,
            temperature: 0.0,
            timeout_ms: 200,
        }
    }

    pub fn planning(prompt: impl Into<String>) -> Self {
        Self {
            kind: InferenceKind::Planning,
            prompt: prompt.into(),
            max_tokens: 512,
            temperature: 0.1,
            timeout_ms: 2000,
        }
    }

    pub fn knowledge(prompt: impl Into<String>) -> Self {
        Self {
            kind: InferenceKind::KnowledgeQuery,
            prompt: prompt.into(),
            max_tokens: 1024,
            temperature: 0.7,
            timeout_ms: 15_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceResponse {
    pub text: String,
    pub tokens_generated: u32,
    pub latency_ms: u64,
    pub timed_out: bool,
}

impl InferenceResponse {
    pub fn timeout() -> Self {
        Self { text: String::new(), tokens_generated: 0, latency_ms: 0, timed_out: true }
    }
    pub fn error(reason: &str) -> Self {
        Self { text: reason.to_string(), tokens_generated: 0, latency_ms: 0, timed_out: false }
    }
}

// ── ModelRuntime trait ────────────────────────────────────────────────────────

/// Abstraction over a loaded local inference model.
///
/// Implementations must be thread-safe (`Send + Sync`).
/// The runtime NEVER calls this from the voice pipeline thread directly —
/// always via a background task or `spawn_blocking`.
pub trait ModelRuntime: Send + Sync {
    fn model_id(&self) -> &str;
    fn is_loaded(&self) -> bool;
    fn supported_kind(&self, kind: &InferenceKind) -> bool;

    /// Synchronous inference. Must respect `request.timeout_ms`.
    /// On timeout: return `InferenceResponse::timeout()`, not an error.
    fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse, String>;

    /// Cancel any in-progress inference (best-effort).
    fn cancel(&self);

    /// RAM usage estimate in MB (for health reporting).
    fn ram_usage_mb(&self) -> u64 { 0 }
}

// ── NullRuntime ───────────────────────────────────────────────────────────────

/// Default runtime when no model is loaded. All inference returns an error.
pub struct NullRuntime;

impl ModelRuntime for NullRuntime {
    fn model_id(&self) -> &str { "null" }
    fn is_loaded(&self) -> bool { false }
    fn supported_kind(&self, _kind: &InferenceKind) -> bool { false }
    fn infer(&self, _req: InferenceRequest) -> Result<InferenceResponse, String> {
        Err("no model loaded".to_string())
    }
    fn cancel(&self) {}
}

// ── InferenceSession ─────────────────────────────────────────────────────────

/// Tracks a single inference call for observability.
pub struct InferenceSession {
    pub request_kind: InferenceKind,
    pub started_ms: u64,
    pub model_id: String,
}

impl InferenceSession {
    pub fn start(kind: InferenceKind, model_id: impl Into<String>) -> Self {
        Self { request_kind: kind, started_ms: now_ms(), model_id: model_id.into() }
    }

    pub fn elapsed_ms(&self) -> u64 {
        now_ms().saturating_sub(self.started_ms)
    }
}

// ── ModelRouter ───────────────────────────────────────────────────────────────

/// Routes inference requests to the appropriate tier based on `InferenceKind`.
///
/// Tier 1 (enrichment) → fastest available runtime
/// Tier 2 (planning)   → reasoning runtime (if loaded)
/// Tier 3 (knowledge)  → heavy runtime (if loaded)
/// Fallback            → NullRuntime (returns error gracefully)
pub struct ModelRouter {
    tier1: Box<dyn ModelRuntime>,
    tier2: Option<Box<dyn ModelRuntime>>,
    tier3: Option<Box<dyn ModelRuntime>>,
}

impl ModelRouter {
    pub fn new_null() -> Self {
        Self {
            tier1: Box::new(NullRuntime),
            tier2: None,
            tier3: None,
        }
    }

    pub fn with_tier1(mut self, rt: Box<dyn ModelRuntime>) -> Self {
        self.tier1 = rt;
        self
    }

    pub fn with_tier2(mut self, rt: Box<dyn ModelRuntime>) -> Self {
        self.tier2 = Some(rt);
        self
    }

    pub fn with_tier3(mut self, rt: Box<dyn ModelRuntime>) -> Self {
        self.tier3 = Some(rt);
        self
    }

    pub fn route(&self, request: InferenceRequest) -> Result<InferenceResponse, String> {
        let runtime: &dyn ModelRuntime = match request.kind {
            InferenceKind::IntentEnrichment | InferenceKind::EmbeddingOnly => self.tier1.as_ref(),
            InferenceKind::Planning => {
                self.tier2.as_deref().unwrap_or(self.tier1.as_ref())
            }
            InferenceKind::KnowledgeQuery => {
                self.tier3.as_deref()
                    .or(self.tier2.as_deref())
                    .unwrap_or(self.tier1.as_ref())
            }
        };

        if !runtime.is_loaded() {
            return Err(format!("no model loaded for {:?}", request.kind));
        }

        let session = InferenceSession::start(request.kind.clone(), runtime.model_id());
        let result = runtime.infer(request);
        debug!("[MODEL_ROUTER] {} elapsed={}ms", session.model_id, session.elapsed_ms());
        result
    }

    pub fn is_any_loaded(&self) -> bool {
        self.tier1.is_loaded()
            || self.tier2.as_ref().map_or(false, |r| r.is_loaded())
            || self.tier3.as_ref().map_or(false, |r| r.is_loaded())
    }

    pub fn tier2_available(&self) -> bool {
        self.tier2.as_ref().map_or(false, |r| r.is_loaded())
    }

    pub fn tier3_available(&self) -> bool {
        self.tier3.as_ref().map_or(false, |r| r.is_loaded())
    }
}
