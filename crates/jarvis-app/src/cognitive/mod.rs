#![allow(dead_code, unused_imports)]

mod domains;
mod intent;
mod memory;
mod planner;
mod tools;
mod clarification;
pub mod metrics;
pub mod state;
pub mod ai_advisor;

// New cognitive runtime modules
pub mod task_memory;
#[cfg(test)]
mod tests;
pub mod context_manager;
pub mod model_runtime;
pub mod trace;
pub mod containment;
pub mod execution_graph;
pub mod router;

pub use domains::Domain;
pub use intent::{EnrichedIntent, Entity, EntityKind, Urgency};
pub use memory::{WorkingMemory, LongTermMemory, ConversationTurn, EpisodicRecord};
pub use planner::{TaskPlan, PlanStep, StepStatus, PlanGraph, PlanEdge, EdgeKind, PlanOrigin, PlanValidator, PlanExecutionBoundary};
pub use tools::{ToolRegistry, ToolCapability, LatencyClass, ToolDescriptor, RetryPolicy, ToolRouter, ToolRouteDecision};
pub use clarification::{ClarificationEngine, ClarificationSession, PendingClarification, ClarificationResolver, ResolveOutcome};
pub use state::CognitiveState;
pub use ai_advisor::{LocalAiAdvisor, NullAdvisor};
pub use task_memory::{TaskMemory, PendingTask, TaskStatus};
pub use context_manager::{ContextManager, ContextSlice, ContextKind};
pub use model_runtime::{ModelRouter, ModelRuntime, NullRuntime, InferenceRequest, InferenceResponse, InferenceKind};
pub use trace::CognitiveTrace;
pub use containment::{HallucinationGuard, ContainmentVerdict};
pub use router::{SemanticRouter, RouteDecision, RoutingContext};

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use jarvis_core::{APP_CONFIG_DIR, APP_LOG_DIR};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Result of cognitive pre-processing — produced before command execution.
pub struct CognitiveResult {
    pub enriched_intent: EnrichedIntent,
    pub plan: TaskPlan,
    /// If set, the command must NOT be executed; ask this question instead.
    pub clarification_needed: Option<(String, Vec<String>)>,
    /// If set, working memory found context that informed this request.
    pub memory_context: Option<String>,
}

/// Central cognitive orchestration runtime.
///
/// Owns working memory, long-term memory, clarification state, and the AI advisor
/// slot. Lives on the app thread — single-threaded access via `&mut`.
pub struct CognitiveRuntime {
    pub working_memory: WorkingMemory,
    pub long_term: LongTermMemory,
    pub clarification: ClarificationEngine,
    pub tool_registry: ToolRegistry,
    pub state: CognitiveState,
    pub task_memory: TaskMemory,
    pub context_manager: ContextManager,
    pub model_router: ModelRouter,
    pub trace: CognitiveTrace,
    pub tool_router: ToolRouter,
    memory_path: PathBuf,
    ai_advisor: Box<dyn LocalAiAdvisor>,
}

impl CognitiveRuntime {
    pub fn new() -> Self {
        let memory_path = APP_CONFIG_DIR.get()
            .map(|d| d.join("cognitive_memory.json"))
            .unwrap_or_else(|| PathBuf::from("cognitive_memory.json"));

        let long_term = LongTermMemory::load(&memory_path);
        info!(
            "[COGNITIVE] Long-term memory loaded: {} episodic records, {} known intents",
            long_term.episodic.len(),
            long_term.preferences.intent_usage.len()
        );
        if let Some(domain) = long_term.preferred_domain() {
            info!("[COGNITIVE] Preferred domain from history: {}", domain);
        }

        let task_path = APP_CONFIG_DIR.get()
            .map(|d| d.join("task_memory.json"))
            .unwrap_or_else(|| PathBuf::from("task_memory.json"));

        Self {
            working_memory: WorkingMemory::new(),
            long_term,
            clarification: ClarificationEngine::new(),
            tool_registry: ToolRegistry::new(),
            state: CognitiveState::Idle,
            task_memory: TaskMemory::new(task_path),
            context_manager: ContextManager::new(),
            model_router: ModelRouter::new_null(),
            trace: CognitiveTrace::new(),
            tool_router: ToolRouter::new(),
            memory_path,
            ai_advisor: Box::new(NullAdvisor),
        }
    }

    /// Replace the AI advisor (called once at startup if a local LLM is available).
    pub fn set_advisor(&mut self, advisor: Box<dyn LocalAiAdvisor>) {
        self.ai_advisor = advisor;
    }

    /// Pre-process a voice utterance through the full cognitive pipeline.
    /// Returns a `CognitiveResult` that drives execution decisions.
    pub fn process(&mut self, text: &str) -> CognitiveResult {
        // 1. Understanding: extract structured intent from raw text.
        self.transition_to(CognitiveState::Understanding);
        let mut enriched = EnrichedIntent::from_text(text);

        // 2. AI advisor enrichment (no-op with NullAdvisor).
        if let Some(ai_enriched) = self.ai_advisor.enrich_intent(text, &enriched, &self.working_memory) {
            enriched = ai_enriched;
        }

        // 3. Reasoning: check working memory for contextual references.
        self.transition_to(CognitiveState::Reasoning);
        let memory_context = self.working_memory.find_context_for(text);

        // Track domain switch.
        if let Some(ref prev) = self.working_memory.active_domain.clone() {
            if *prev != enriched.domain && !matches!(enriched.domain, Domain::Unknown) {
                metrics::DOMAIN_SWITCHES.fetch_add(1, Ordering::Relaxed);
            }
        }
        if memory_context.is_some() {
            metrics::MEMORY_HITS.fetch_add(1, Ordering::Relaxed);
            metrics::CONTEXT_RESOLUTIONS.fetch_add(1, Ordering::Relaxed);
        } else {
            metrics::MEMORY_MISSES.fetch_add(1, Ordering::Relaxed);
        }

        // 4. Planning: check if clarification is needed, then build plan.
        self.transition_to(CognitiveState::Planning);
        let clarification_needed = self.clarification.check(&enriched);
        if clarification_needed.is_some() {
            metrics::CLARIFICATIONS_ISSUED.fetch_add(1, Ordering::Relaxed);
        }

        // Plan: let the AI advisor try multi-step; fall back to single-step.
        let plan = self.ai_advisor
            .generate_plan(text, &[], &self.working_memory)
            .unwrap_or_else(|| TaskPlan::single(text, text));

        self.trace.log_routing(
            text,
            enriched.domain.as_str(),
            if clarification_needed.is_some() { "clarify" } else { "execute" },
            enriched.confidence,
            0,
        );

        metrics::GOAL_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

        CognitiveResult {
            enriched_intent: enriched,
            plan,
            clarification_needed,
            memory_context,
        }
    }

    /// Record the outcome of a command execution.
    /// Called after execute_command returns.
    pub fn observe(&mut self, intent: &EnrichedIntent, success: bool) {
        self.transition_to(CognitiveState::Observing);

        let turn = ConversationTurn {
            text: intent.raw_text.clone(),
            domain: intent.domain.clone(),
            intent_id: intent.matched_intent_id.clone(),
            entities: intent.entities.iter().map(|e| e.value.clone()).collect(),
            success,
            timestamp_ms: now_ms(),
        };

        self.working_memory.push(turn.clone());
        self.long_term.record_episodic(&turn);
        self.long_term.update_preference(
            intent.domain.as_str(),
            intent.matched_intent_id.as_deref(),
            turn.timestamp_ms,
        );
        self.long_term.save(&self.memory_path);

        self.trace.log_execution(
            &intent.raw_text,
            intent.domain.as_str(),
            success,
            0,
        );

        if success {
            metrics::GOAL_SUCCESSES.fetch_add(1, Ordering::Relaxed);
        } else {
            metrics::GOAL_FAILURES.fetch_add(1, Ordering::Relaxed);
        }

        self.transition_to(CognitiveState::Idle);
    }

    /// Called when the voice session ends (STT worker entered Idle/Cooldown).
    pub fn on_session_end(&mut self) {
        self.working_memory.reset_session();
        self.transition_to(CognitiveState::Idle);
    }

    /// Called when the user provides an answer to a pending clarification.
    pub fn resolve_clarification(&mut self, answer: &str) {
        if self.clarification.take_pending().is_some() {
            metrics::CLARIFICATIONS_RESOLVED.fetch_add(1, Ordering::Relaxed);
            info!("[COGNITIVE] Clarification resolved: '{}'", answer);
        }
    }

    /// Emit a structured `[COGNITION]` log line for observability.
    pub fn log_decision(
        &self,
        goal: &str,
        domain: &Domain,
        reasoning: &str,
        result: &str,
    ) {
        info!(
            "[COGNITION] goal=\"{}\" domain={} reasoning=\"{}\" result={}",
            goal,
            domain.as_str(),
            reasoning,
            result
        );
    }

    /// Emit a periodic cognitive health report.
    pub fn emit_health_report(&self) {
        info!(
            "[COGNITIVE][HEALTH] goals={}/{} success_rate={:.1}% memory_hit_rate={:.1}% \
             clarification_rate={:.1}% domain_switches={} context_resolutions={}",
            metrics::GOAL_SUCCESSES.load(Ordering::Relaxed),
            metrics::GOAL_ATTEMPTS.load(Ordering::Relaxed),
            metrics::goal_success_rate() * 100.0,
            metrics::memory_hit_rate() * 100.0,
            metrics::clarification_rate() * 100.0,
            metrics::DOMAIN_SWITCHES.load(Ordering::Relaxed),
            metrics::CONTEXT_RESOLUTIONS.load(Ordering::Relaxed),
        );
    }

    fn transition_to(&mut self, next: CognitiveState) {
        if self.state != next {
            debug!("[COGNITION] {} → {}", self.state.as_str(), next.as_str());
            self.state = next;
        }
    }
}
