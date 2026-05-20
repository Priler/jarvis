#![allow(dead_code)]

use std::time::SystemTime;
use super::intent::{EnrichedIntent, Urgency};
use super::domains::Domain;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// Clarification sessions expire after 30 seconds with no response.
const SESSION_EXPIRY_MS: u64 = 30_000;

// ── Clarification session ─────────────────────────────────────────────────────

/// A single pending clarification request, keyed by session ID.
#[derive(Debug, Clone)]
pub struct PendingClarification {
    pub question: String,
    pub options: Vec<String>,
    pub original_text: String,
}

/// Active clarification session.
#[derive(Debug)]
pub struct ClarificationSession {
    pub id: String,
    pub pending: PendingClarification,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub rounds: u8,
}

impl ClarificationSession {
    pub fn new(id: impl Into<String>, pending: PendingClarification) -> Self {
        let now = now_ms();
        Self {
            id: id.into(),
            pending,
            created_ms: now,
            expires_ms: now + SESSION_EXPIRY_MS,
            rounds: 1,
        }
    }

    pub fn is_expired(&self) -> bool {
        now_ms() > self.expires_ms
    }

    pub fn renew(&mut self) {
        self.expires_ms = now_ms() + SESSION_EXPIRY_MS;
        self.rounds += 1;
    }
}

// ── Clarification resolver ─────────────────────────────────────────────────────

/// Result of resolving a clarification answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOutcome {
    /// The answer matched an option; resolved command text is returned.
    Resolved(String),
    /// Answer did not match any option; session remains open.
    Unresolved,
    /// No active session or session expired.
    NoSession,
}

pub struct ClarificationResolver;

impl ClarificationResolver {
    /// Try to match a user's free-form `answer` against the pending options.
    /// Returns `Resolved(command)` if a match is found.
    pub fn resolve(session: &ClarificationSession, answer: &str) -> ResolveOutcome {
        if session.is_expired() {
            return ResolveOutcome::NoSession;
        }

        let lower = answer.to_lowercase();

        // Direct option match (case-insensitive substring).
        for opt in &session.pending.options {
            if lower.contains(&opt.to_lowercase()) {
                let command = format!("{} {}", session.pending.original_text, opt);
                return ResolveOutcome::Resolved(command);
            }
        }

        // Ordinal match: "first", "second", "1", "2", etc.
        let ordinals = [("first", 0usize), ("second", 1), ("third", 2), ("1", 0), ("2", 1), ("3", 2)];
        for (word, idx) in &ordinals {
            if lower.contains(word) {
                if let Some(opt) = session.pending.options.get(*idx) {
                    let command = format!("{} {}", session.pending.original_text, opt);
                    return ResolveOutcome::Resolved(command);
                }
            }
        }

        ResolveOutcome::Unresolved
    }
}

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
