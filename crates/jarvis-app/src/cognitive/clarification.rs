#![allow(dead_code)]

use super::intent::{EnrichedIntent, Urgency};
use super::domains::Domain;

pub struct ClarificationContext {
    pub question: String,
    pub options: Vec<String>,
    pub original_text: String,
    pub expires_at_ms: u64,
}

pub struct ClarificationEngine {
    pub pending: Option<ClarificationContext>,
}

impl ClarificationEngine {
    pub fn new() -> Self {
        Self { pending: None }
    }

    /// Returns `Some((question, options))` when the intent is ambiguous enough to need clarification.
    /// Returns `None` to proceed with execution.
    pub fn check(&self, intent: &EnrichedIntent) -> Option<(String, Vec<String>)> {
        // Never delay urgent commands.
        if intent.urgency == Urgency::High {
            return None;
        }

        // Complete noise: no domain, very low confidence.
        if matches!(intent.domain, Domain::Unknown) && intent.confidence < 0.35 {
            return Some((
                "Извините, я не понял. Повторите команду.".to_string(),
                vec![],
            ));
        }

        // Media ambiguity: user wants to play something but target is unclear.
        if matches!(intent.domain, Domain::Media) && intent.confidence < 0.55 {
            let text = &intent.normalized_text;
            let has_target = text.contains("youtube") || text.contains("spotify")
                || text.contains("local") || text.contains("плеер");
            if !has_target
                && (text.contains("play") || text.contains("включи") || text.contains("запусти"))
            {
                return Some((
                    "Включить на локальном плеере или онлайн?".to_string(),
                    vec!["Локальный плеер".to_string(), "YouTube".to_string()],
                ));
            }
        }

        None
    }

    pub fn set_pending(
        &mut self,
        question: String,
        options: Vec<String>,
        original_text: String,
        expires_at_ms: u64,
    ) {
        self.pending = Some(ClarificationContext { question, options, original_text, expires_at_ms });
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.pending.as_ref().map_or(false, |p| now_ms > p.expires_at_ms)
    }

    pub fn take_pending(&mut self) -> Option<ClarificationContext> {
        self.pending.take()
    }
}
