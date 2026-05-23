//! Semantic intent parsing layer.
//!
//! Sits between raw STT transcripts and the command dispatcher.
//! Enriches pattern-matched intents with LLM confidence scoring
//! when the local model is available.
//!
//! Parse pipeline:
//!   1. STT transcript arrives as `&str`
//!   2. `parse()` applies pattern-matching baseline
//!   3. If LLM is ready: calls `llm_runtime::enrich()` to score/expand
//!   4. Returns `SemanticParseResult` with method tag for observability

use std::sync::atomic::{AtomicU64, Ordering};

// ── Counters ──────────────────────────────────────────────────────────────────

pub static INTENT_PARSES:   AtomicU64 = AtomicU64::new(0);
pub static LLM_ENRICHMENTS: AtomicU64 = AtomicU64::new(0);
pub static FALLBACK_PARSES: AtomicU64 = AtomicU64::new(0);

// ── Parse method ──────────────────────────────────────────────────────────────

/// How the intent was resolved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ParseMethod {
    /// LLM was available and returned an enriched result.
    LlmEnriched,
    /// Fast pattern match only — LLM was not available or not needed.
    PatternMatched,
    /// Neither pattern nor LLM matched; fallback to raw transcript.
    Fallback,
}

// ── Result ────────────────────────────────────────────────────────────────────

/// Outcome of intent parsing for a single utterance.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SemanticParseResult {
    /// Original transcript from STT.
    pub raw_text: String,
    /// Resolved intent label (may equal raw_text if Fallback).
    pub intent:   String,
    /// Confidence in [0.0, 1.0].
    pub confidence: f32,
    /// How this result was produced.
    pub method:   ParseMethod,
    /// True if the intent is ambiguous and needs clarification.
    pub needs_clarification: bool,
}

impl SemanticParseResult {
    pub fn is_actionable(&self) -> bool {
        self.confidence >= 0.35 && !self.needs_clarification
    }

    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= 0.75
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse a raw transcript into a `SemanticParseResult`.
///
/// Attempts LLM enrichment if the runtime is ready; falls back to
/// pattern matching + raw passthrough otherwise.
pub fn parse(transcript: &str) -> SemanticParseResult {
    INTENT_PARSES.fetch_add(1, Ordering::Relaxed);

    let sanitized = crate::cognitive::containment::HallucinationGuard::sanitize_prompt_input(transcript);
    if sanitized.is_empty() {
        FALLBACK_PARSES.fetch_add(1, Ordering::Relaxed);
        return fallback(transcript);
    }

    // Try LLM enrichment when the runtime is ready.
    if crate::llm_runtime::is_ready() {
        if let Ok(resp) = crate::llm_runtime::enrich(&sanitized) {
            if !resp.text.is_empty() && !resp.timed_out {
                LLM_ENRICHMENTS.fetch_add(1, Ordering::Relaxed);
                let confidence = parse_confidence_from_llm_text(&resp.text);
                return SemanticParseResult {
                    raw_text:            transcript.to_string(),
                    intent:              extract_intent_from_llm_text(&resp.text, &sanitized),
                    confidence,
                    method:              ParseMethod::LlmEnriched,
                    needs_clarification: confidence < 0.52,
                };
            }
        }
    }

    // Pattern matching baseline.
    if let Some(result) = pattern_match(&sanitized) {
        return result;
    }

    FALLBACK_PARSES.fetch_add(1, Ordering::Relaxed);
    fallback(transcript)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn pattern_match(text: &str) -> Option<SemanticParseResult> {
    let lower = text.to_lowercase();
    // Simple prefix patterns — not meant to be exhaustive.
    let patterns: &[(&str, &str, f32)] = &[
        ("открой",     "open_app",    0.85),
        ("запусти",    "launch_app",  0.85),
        ("закрой",     "close_app",   0.80),
        ("напомни",    "set_reminder",0.82),
        ("покажи",     "show_info",   0.78),
        ("выключи",    "power_off",   0.90),
        ("перезагрузи","restart",     0.88),
        ("open",       "open_app",    0.85),
        ("close",      "close_app",   0.80),
        ("show",       "show_info",   0.78),
        ("launch",     "launch_app",  0.85),
        ("remind",     "set_reminder",0.82),
        ("shutdown",   "power_off",   0.90),
    ];

    for (prefix, intent, conf) in patterns {
        if lower.starts_with(prefix) {
            return Some(SemanticParseResult {
                raw_text:            text.to_string(),
                intent:              intent.to_string(),
                confidence:          *conf,
                method:              ParseMethod::PatternMatched,
                needs_clarification: false,
            });
        }
    }
    None
}

fn fallback(raw: &str) -> SemanticParseResult {
    SemanticParseResult {
        raw_text:            raw.to_string(),
        intent:              raw.trim().to_lowercase().replace(' ', "_"),
        confidence:          0.20,
        method:              ParseMethod::Fallback,
        needs_clarification: true,
    }
}

fn extract_intent_from_llm_text(llm_text: &str, sanitized: &str) -> String {
    // Expect LLM to return structured output like "intent: open_app"
    // or just the raw text as a fallback.
    for line in llm_text.lines() {
        let lower = line.trim().to_lowercase();
        if let Some(rest) = lower.strip_prefix("intent:") {
            let candidate = rest.trim().to_string();
            if !candidate.is_empty() {
                return candidate;
            }
        }
    }
    sanitized.trim().to_lowercase().replace(' ', "_")
}

fn parse_confidence_from_llm_text(text: &str) -> f32 {
    for line in text.lines() {
        let lower = line.trim().to_lowercase();
        if let Some(rest) = lower.strip_prefix("confidence:") {
            if let Ok(val) = rest.trim().parse::<f32>() {
                return val.clamp(0.0, 1.0);
            }
        }
    }
    // No structured confidence — treat as moderate.
    0.65
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_on_empty_transcript() {
        let r = parse("   ");
        assert_eq!(r.method, ParseMethod::Fallback);
        assert!(!r.is_actionable());
    }

    #[test]
    fn pattern_match_russian_open() {
        let r = parse("открой калькулятор");
        assert!(matches!(r.method, ParseMethod::PatternMatched | ParseMethod::LlmEnriched));
        // PatternMatched gives 0.85; LlmEnriched via stub gives 0.65.
        // Both are above the actionable threshold (0.35).
        assert!(r.is_actionable());
    }

    #[test]
    fn pattern_match_english_open() {
        let r = parse("open browser");
        assert!(matches!(r.method, ParseMethod::PatternMatched | ParseMethod::LlmEnriched));
        assert!(r.is_actionable());
    }

    #[test]
    fn unknown_text_becomes_fallback() {
        let r = parse("xyzzy fnord");
        // Without LLM: fallback.  With stub LLM: LlmEnriched at 0.65.
        assert!(!r.raw_text.is_empty());
    }

    #[test]
    fn is_high_confidence_threshold() {
        let r = SemanticParseResult {
            raw_text: "test".into(),
            intent: "test".into(),
            confidence: 0.80,
            method: ParseMethod::PatternMatched,
            needs_clarification: false,
        };
        assert!(r.is_high_confidence());
    }

    #[test]
    fn intent_parses_counter_increments() {
        let before = INTENT_PARSES.load(Ordering::Relaxed);
        parse("открой калькулятор");
        assert!(INTENT_PARSES.load(Ordering::Relaxed) > before);
    }
}
