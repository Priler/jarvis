#![allow(dead_code)]

use super::intent::EnrichedIntent;
use super::memory::{WorkingMemory, ConversationTurn};
use super::planner::TaskPlan;
use super::tools::ToolCapability;

/// Interface for a local AI model that can enrich cognitive processing.
///
/// All methods default to returning `None` (no enrichment). Swap in a local LLM
/// implementation without touching any other layer.
///
/// Safety contract: the advisor MUST NOT own any runtime state, issue IPC events,
/// or execute commands directly. It is a pure cognitive advisor — it returns data
/// that the CognitiveRuntime uses to make decisions.
pub trait LocalAiAdvisor: Send + Sync {
    /// Optionally enrich an already-extracted intent using working memory context.
    fn enrich_intent(
        &self,
        _text: &str,
        _intent: &EnrichedIntent,
        _memory: &WorkingMemory,
    ) -> Option<EnrichedIntent> {
        None
    }

    /// Optionally generate a multi-step plan from a natural-language goal.
    fn generate_plan(
        &self,
        _goal: &str,
        _available_tools: &[ToolCapability],
        _memory: &WorkingMemory,
    ) -> Option<TaskPlan> {
        None
    }

    /// Optionally resolve a pronoun reference ("it", "that") from conversation history.
    fn resolve_reference(
        &self,
        _text: &str,
        _history: &[ConversationTurn],
    ) -> Option<String> {
        None
    }

    /// Optionally generate a more contextual clarification question than the rule-based engine.
    fn generate_clarification(&self, _intent: &EnrichedIntent) -> Option<String> {
        None
    }
}

/// No-op advisor used when no local LLM is configured.
pub struct NullAdvisor;

impl LocalAiAdvisor for NullAdvisor {}
