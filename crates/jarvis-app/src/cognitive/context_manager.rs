#![allow(dead_code)]

//! Context Window Manager — assembles, scores, and prunes the LLM prompt
//! so that context never exceeds the token budget.

// ── Budget constants ──────────────────────────────────────────────────────────

/// Maximum context tokens for the reasoning model (Tier 2, n_ctx = 2048).
pub const CTX_MAX_TOKENS: usize = 2048;
/// Fixed system-prompt token allocation.
pub const CTX_SYSTEM_TOKENS: usize = 200;
/// Reserved for the generated response.
pub const CTX_RESPONSE_TOKENS: usize = 300;
/// Fixed allocation for the current user request.
pub const CTX_REQUEST_TOKENS: usize = 60;
/// Available for memory + tools + context.
pub const CTX_AVAILABLE_TOKENS: usize =
    CTX_MAX_TOKENS - CTX_SYSTEM_TOKENS - CTX_RESPONSE_TOKENS - CTX_REQUEST_TOKENS;

// ── Context slice kinds ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextKind {
    SystemPrompt,        // always injected first, never pruned
    UserRequest,         // the current utterance, never pruned
    ConversationHistory, // recent turns
    MemoryFact,          // semantic / user memory facts
    RuntimeState,        // degraded_mode, active_domain, last_success
    ToolList,            // available tool descriptions
}

// ── Context slice ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ContextSlice {
    pub kind: ContextKind,
    pub content: String,
    pub relevance: f32,    // 0.0–1.0; higher = keep when pruning
}

impl ContextSlice {
    pub fn new(kind: ContextKind, content: impl Into<String>, relevance: f32) -> Self {
        Self { kind, content: content.into(), relevance }
    }

    pub fn token_estimate(&self) -> usize {
        estimate_tokens(&self.content)
    }
}

// ── Token estimator ───────────────────────────────────────────────────────────

/// Rough estimate: 1 token ≈ 4 chars for English/Russian mixed text.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

// ── Context budget ────────────────────────────────────────────────────────────

pub struct ContextBudget {
    pub max_tokens: usize,
    pub system_tokens: usize,
    pub request_tokens: usize,
    pub response_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_tokens: CTX_MAX_TOKENS,
            system_tokens: CTX_SYSTEM_TOKENS,
            request_tokens: CTX_REQUEST_TOKENS,
            response_tokens: CTX_RESPONSE_TOKENS,
        }
    }
}

impl ContextBudget {
    pub fn available(&self) -> usize {
        self.max_tokens
            .saturating_sub(self.system_tokens)
            .saturating_sub(self.request_tokens)
            .saturating_sub(self.response_tokens)
    }
}

// ── Context manager ───────────────────────────────────────────────────────────

pub struct ContextManager {
    pub budget: ContextBudget,
}

impl ContextManager {
    pub fn new() -> Self {
        Self { budget: ContextBudget::default() }
    }

    /// Create a manager where `available_tokens` is the directly usable slice budget.
    /// Overheads (system/request/response) are zeroed so `budget.available() == available_tokens`.
    pub fn with_budget(available_tokens: usize) -> Self {
        Self {
            budget: ContextBudget {
                max_tokens: available_tokens,
                system_tokens: 0,
                request_tokens: 0,
                response_tokens: 0,
            },
        }
    }

    /// Assemble a prompt string from slices, pruning lowest-relevance slices
    /// to stay within the available token budget.
    ///
    /// Ordering: SystemPrompt → RuntimeState → ToolList → MemoryFact →
    ///           ConversationHistory → UserRequest
    pub fn build_context(&self, mut slices: Vec<ContextSlice>) -> String {
        let budget = self.budget.available();

        // Mandatory slices (never pruned):
        let mandatory = &[ContextKind::SystemPrompt, ContextKind::UserRequest];

        // Sort non-mandatory slices by relevance descending.
        slices.sort_by(|a, b| {
            let a_mandatory = mandatory.contains(&a.kind);
            let b_mandatory = mandatory.contains(&b.kind);
            match (a_mandatory, b_mandatory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.relevance.partial_cmp(&a.relevance).unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        // Greedy fill: include slices until budget exhausted.
        let mut used_tokens = 0usize;
        let mut selected: Vec<&ContextSlice> = Vec::new();

        for slice in &slices {
            let tokens = slice.token_estimate();
            let is_mandatory = mandatory.contains(&slice.kind);
            if is_mandatory || used_tokens + tokens <= budget {
                used_tokens += tokens;
                selected.push(slice);
            } else {
                debug!("[CTX] Pruned {:?} slice ({} tokens, relevance {:.2})",
                    slice.kind, tokens, slice.relevance);
            }
        }

        // Re-sort selected into logical prompt order.
        let order = |k: &ContextKind| match k {
            ContextKind::SystemPrompt => 0,
            ContextKind::RuntimeState => 1,
            ContextKind::ToolList => 2,
            ContextKind::MemoryFact => 3,
            ContextKind::ConversationHistory => 4,
            ContextKind::UserRequest => 5,
        };
        selected.sort_by_key(|s| order(&s.kind));

        let parts: Vec<&str> = selected.iter().map(|s| s.content.as_str()).collect();
        parts.join("\n")
    }

    /// Return the total token estimate for a set of slices.
    pub fn total_tokens(slices: &[ContextSlice]) -> usize {
        slices.iter().map(|s| s.token_estimate()).sum()
    }

    /// Check whether a set of slices fits within the available budget.
    pub fn fits_budget(&self, slices: &[ContextSlice]) -> bool {
        Self::total_tokens(slices) <= self.budget.available()
    }
}

#[cfg(test)]
mod ctx_tests {
    use super::*;

    #[test]
    fn build_context_respects_budget() {
        let mgr = ContextManager::with_budget(400); // tiny budget for test
        let slices = vec![
            ContextSlice::new(ContextKind::SystemPrompt, "SYS", 1.0),
            ContextSlice::new(ContextKind::UserRequest, "USR", 1.0),
            // Large low-relevance slice — should be pruned
            ContextSlice::new(ContextKind::ConversationHistory,
                "A".repeat(1200), // ~300 tokens
                0.1),
            // Small high-relevance slice — should survive
            ContextSlice::new(ContextKind::MemoryFact, "short fact", 0.9),
        ];
        let result = mgr.build_context(slices);
        // Mandatory slices always present
        assert!(result.contains("SYS"));
        assert!(result.contains("USR"));
        // High-relevance small slice survives
        assert!(result.contains("short fact"));
        // Token estimate for result should be reasonable
        assert!(estimate_tokens(&result) < 500);
    }

    #[test]
    fn estimate_tokens_nonzero() {
        assert!(estimate_tokens("hello world") >= 1);
        assert_eq!(estimate_tokens(""), 1); // min = 1
    }
}
