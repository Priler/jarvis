//! Intent confidence modeling.
//!
//! Aggregates signals from pattern matching and LLM enrichment into a
//! single confidence model used by the semantic router and clarification
//! engine to decide whether to execute, clarify, or reject.

use crate::semantic_intent::{ParseMethod, SemanticParseResult};

// ── Thresholds (mirrors cognitive::router thresholds) ─────────────────────────

/// Accept and execute directly.
pub const CONF_EXECUTE:   f32 = 0.75;
/// Acceptable but trigger planning before execution.
pub const CONF_PLAN:      f32 = 0.52;
/// Ambiguous — ask a clarifying question.
pub const CONF_CLARIFY:   f32 = 0.35;
// Below CONF_CLARIFY → reject.

// ── Confidence model ──────────────────────────────────────────────────────────

/// Structured confidence output for a parsed intent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IntentConfidenceModel {
    /// Aggregated confidence in [0.0, 1.0].
    pub score:                f32,
    /// True if multiple interpretations are plausible.
    pub ambiguous:            bool,
    /// Estimated probability that the intent will fail to execute.
    pub fallback_probability: f32,
    /// True if the system should ask a clarifying question.
    pub needs_clarification:  bool,
    /// Routing decision derived from score.
    pub decision:             ConfidenceDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ConfidenceDecision {
    Execute,
    Plan,
    Clarify,
    Reject,
}

impl IntentConfidenceModel {
    /// Build a model from a `SemanticParseResult`.
    pub fn from_parse(result: &SemanticParseResult) -> Self {
        let score = result.confidence;
        let ambiguous = result.needs_clarification || score < CONF_PLAN;

        let fallback_probability = match result.method {
            ParseMethod::LlmEnriched    => (1.0 - score) * 0.5,
            ParseMethod::PatternMatched => (1.0 - score) * 0.6,
            ParseMethod::Fallback       => 0.95,
        };

        let needs_clarification = result.needs_clarification || score < CONF_CLARIFY;

        let decision = if score >= CONF_EXECUTE {
            ConfidenceDecision::Execute
        } else if score >= CONF_PLAN {
            ConfidenceDecision::Plan
        } else if score >= CONF_CLARIFY {
            ConfidenceDecision::Clarify
        } else {
            ConfidenceDecision::Reject
        };

        Self { score, ambiguous, fallback_probability, needs_clarification, decision }
    }

    /// Build from a raw score without a parse result (for testing/fallback).
    pub fn from_score(score: f32) -> Self {
        let score = score.clamp(0.0, 1.0);
        let decision = if score >= CONF_EXECUTE {
            ConfidenceDecision::Execute
        } else if score >= CONF_PLAN {
            ConfidenceDecision::Plan
        } else if score >= CONF_CLARIFY {
            ConfidenceDecision::Clarify
        } else {
            ConfidenceDecision::Reject
        };
        Self {
            score,
            ambiguous: score < CONF_PLAN,
            fallback_probability: (1.0 - score).max(0.0),
            needs_clarification: score < CONF_CLARIFY,
            decision,
        }
    }

    pub fn is_actionable(&self) -> bool {
        matches!(self.decision, ConfidenceDecision::Execute | ConfidenceDecision::Plan)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_confidence_routes_to_execute() {
        let m = IntentConfidenceModel::from_score(0.90);
        assert_eq!(m.decision, ConfidenceDecision::Execute);
        assert!(m.is_actionable());
    }

    #[test]
    fn mid_confidence_routes_to_plan() {
        let m = IntentConfidenceModel::from_score(0.60);
        assert_eq!(m.decision, ConfidenceDecision::Plan);
        assert!(m.is_actionable());
    }

    #[test]
    fn low_confidence_routes_to_clarify() {
        let m = IntentConfidenceModel::from_score(0.40);
        assert_eq!(m.decision, ConfidenceDecision::Clarify);
        assert!(!m.is_actionable());
    }

    #[test]
    fn very_low_confidence_routes_to_reject() {
        let m = IntentConfidenceModel::from_score(0.10);
        assert_eq!(m.decision, ConfidenceDecision::Reject);
    }

    #[test]
    fn from_parse_pattern_matched() {
        let r = SemanticParseResult {
            raw_text: "open calculator".into(),
            intent: "open_app".into(),
            confidence: 0.85,
            method: ParseMethod::PatternMatched,
            needs_clarification: false,
        };
        let m = IntentConfidenceModel::from_parse(&r);
        assert_eq!(m.decision, ConfidenceDecision::Execute);
        assert!(!m.needs_clarification);
    }

    #[test]
    fn from_parse_fallback_is_not_actionable() {
        let r = SemanticParseResult {
            raw_text: "xyzzy".into(),
            intent: "xyzzy".into(),
            confidence: 0.20,
            method: ParseMethod::Fallback,
            needs_clarification: true,
        };
        let m = IntentConfidenceModel::from_parse(&r);
        assert!(!m.is_actionable());
        assert!(m.fallback_probability > 0.5);
    }
}
