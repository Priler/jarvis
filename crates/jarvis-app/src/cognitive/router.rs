#![allow(dead_code)]

//! Semantic router — maps an EnrichedIntent to an execution decision.
//!
//! Routing is purely deterministic: rule-based first, then confidence
//! thresholds. The LLM advisor is consulted only when confidence falls
//! below CONF_LLM_THRESHOLD and a local model is available.
//!
//! Decision order:
//!   1. Containment block? → Reject
//!   2. High urgency + known domain → Execute directly
//!   3. Domain::Unknown AND confidence < CONF_REJECT → Reject (no command)
//!   4. confidence ≥ CONF_DIRECT → Execute directly
//!   5. confidence ≥ CONF_PLAN → Route to planner
//!   6. confidence < CONF_CLARIFY → Request clarification
//!   7. Fallback → Execute (best-effort)

use super::intent::{EnrichedIntent, Urgency};
use super::domains::Domain;
use super::containment::HallucinationGuard;

// ── Thresholds ─────────────────────────────────────────────────────────────────

/// Above this: execute directly without planning.
pub const CONF_DIRECT: f64 = 0.75;
/// Above this but below CONF_DIRECT: route to planner.
pub const CONF_PLAN: f64 = 0.52;
/// Below this for Unknown domain: reject and ask for repeat.
pub const CONF_REJECT: f64 = 0.35;
/// Below CONF_PLAN but non-reject: request clarification.
pub const CONF_CLARIFY: f64 = 0.52;

// ── Routing context ───────────────────────────────────────────────────────────

/// Snapshot of runtime state fed into the routing decision.
#[derive(Debug, Clone)]
pub struct RoutingContext {
    /// Whether the system is in degraded mode (LLM advisor unavailable).
    pub degraded: bool,
    /// Number of consecutive clarification rounds without resolution.
    pub pending_clarification_rounds: u8,
    /// Whether a plan is currently active.
    pub plan_active: bool,
}

impl Default for RoutingContext {
    fn default() -> Self {
        Self { degraded: false, pending_clarification_rounds: 0, plan_active: false }
    }
}

// ── Route decision ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// Execute the command text directly via governance → runtime.
    Execute,
    /// Expand into a multi-step plan via the planner.
    Plan,
    /// Ask the user for clarification before proceeding.
    Clarify { question: String, options: Vec<String> },
    /// Do not execute; log and surface reason to user.
    Reject { reason: String },
}

// ── Semantic router ───────────────────────────────────────────────────────────

pub struct SemanticRouter;

impl SemanticRouter {
    /// Produce a `RouteDecision` for the given intent and runtime context.
    pub fn route(intent: &EnrichedIntent, ctx: &RoutingContext) -> RouteDecision {
        let text = &intent.normalized_text;

        // R1: Containment — reject if the text contains injection markers.
        if !HallucinationGuard::check_command_text(text).is_safe() {
            return RouteDecision::Reject {
                reason: "input failed containment check".to_string(),
            };
        }

        // R2: High urgency + known domain → execute immediately.
        if intent.urgency == Urgency::High && !matches!(intent.domain, Domain::Unknown) {
            return RouteDecision::Execute;
        }

        // R3: Unknown domain AND very low confidence → reject.
        if matches!(intent.domain, Domain::Unknown) && intent.confidence < CONF_REJECT {
            return RouteDecision::Reject {
                reason: "unknown domain and low confidence — command not understood".to_string(),
            };
        }

        // R4: Max clarification rounds exceeded → fallback to execute.
        if ctx.pending_clarification_rounds >= 3 {
            return RouteDecision::Execute;
        }

        // R5: High confidence → execute directly.
        if intent.confidence >= CONF_DIRECT {
            return RouteDecision::Execute;
        }

        // R6: Medium confidence → route to planner (unless in degraded mode).
        if intent.confidence >= CONF_PLAN {
            if ctx.plan_active {
                // Already planning; execute current step, don't re-plan.
                return RouteDecision::Execute;
            }
            // Multi-domain or multi-entity intents benefit from planning.
            if Self::needs_planning(intent) {
                return RouteDecision::Plan;
            }
            return RouteDecision::Execute;
        }

        // R7: Low confidence + media ambiguity → clarify.
        if matches!(intent.domain, Domain::Media) && intent.confidence < CONF_CLARIFY {
            return Self::media_clarification(intent);
        }

        // R8: Unknown domain → clarify (not enough signal to reject or plan).
        if matches!(intent.domain, Domain::Unknown) {
            return RouteDecision::Clarify {
                question: "Команда не распознана. Повторите?".to_string(),
                options: vec![],
            };
        }

        // R9: Fallback — best-effort execute.
        RouteDecision::Execute
    }

    fn needs_planning(intent: &EnrichedIntent) -> bool {
        // Multi-entity intents often imply a sequence.
        if intent.entities.len() >= 2 {
            return true;
        }
        // System domain commands with targets suggest multi-step.
        if matches!(intent.domain, Domain::System) && !intent.entities.is_empty() {
            return true;
        }
        false
    }

    fn media_clarification(intent: &EnrichedIntent) -> RouteDecision {
        let text = &intent.normalized_text;
        let has_target = text.contains("youtube") || text.contains("spotify")
            || text.contains("local") || text.contains("плеер");

        if !has_target {
            return RouteDecision::Clarify {
                question: "Включить на локальном плеере или онлайн?".to_string(),
                options: vec!["Локальный плеер".to_string(), "YouTube".to_string()],
            };
        }

        RouteDecision::Execute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_intent(domain: Domain, confidence: f64, urgency: Urgency) -> EnrichedIntent {
        EnrichedIntent {
            raw_text: "test".into(),
            normalized_text: "test command".into(),
            domain,
            entities: vec![],
            urgency,
            context_dependent: false,
            matched_intent_id: None,
            confidence,
        }
    }

    #[test]
    fn high_urgency_executes_directly() {
        let intent = make_intent(Domain::System, 0.3, Urgency::High);
        let ctx = RoutingContext::default();
        assert_eq!(SemanticRouter::route(&intent, &ctx), RouteDecision::Execute);
    }

    #[test]
    fn unknown_domain_low_conf_rejected() {
        let intent = make_intent(Domain::Unknown, 0.2, Urgency::Normal);
        let ctx = RoutingContext::default();
        assert!(matches!(SemanticRouter::route(&intent, &ctx), RouteDecision::Reject { .. }));
    }

    #[test]
    fn high_confidence_executes() {
        let intent = make_intent(Domain::System, 0.9, Urgency::Normal);
        let ctx = RoutingContext::default();
        assert_eq!(SemanticRouter::route(&intent, &ctx), RouteDecision::Execute);
    }
}
