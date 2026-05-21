//! Multi-turn LLM session context management.
//!
//! Maintains a conversation history (system prompt + user/assistant turns)
//! and builds the flat prompt string consumed by local inference backends.
//!
//! The session enforces a maximum turn count to prevent context overflow.
//! Old turns are evicted from the front when the limit is exceeded.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

pub static SESSIONS_CREATED: AtomicU64 = AtomicU64::new(0);
static SESSION_ID_SEQ: AtomicU64 = AtomicU64::new(1);

const DEFAULT_MAX_TURNS: usize = 20;

// ── Role ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

impl LlmRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmRole::System    => "system",
            LlmRole::User      => "user",
            LlmRole::Assistant => "assistant",
        }
    }
}

// ── Turn ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize)]
pub struct LlmTurn {
    pub role:      LlmRole,
    pub content:   String,
    pub ts_ms:     u64,
}

// ── Session ───────────────────────────────────────────────────────────────────

/// A single multi-turn conversation session with a local LLM.
///
/// Sessions are created per voice session — not long-lived across reboots.
/// Memory is bounded by `max_turns` (default 20 turns, ~10 exchanges).
pub struct LlmSession {
    pub id:            u64,
    pub system_prompt: String,
    pub turns:         Vec<LlmTurn>,
    max_turns:         usize,
}

impl LlmSession {
    pub fn new(system_prompt: impl Into<String>) -> Self {
        let id = SESSION_ID_SEQ.fetch_add(1, Ordering::Relaxed);
        SESSIONS_CREATED.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            system_prompt: system_prompt.into(),
            turns: Vec::new(),
            max_turns: DEFAULT_MAX_TURNS,
        }
    }

    pub fn with_max_turns(mut self, n: usize) -> Self {
        self.max_turns = n.max(2);
        self
    }

    /// Append a user message.
    pub fn push_user(&mut self, content: impl Into<String>) {
        self.push(LlmRole::User, content.into());
    }

    /// Append an assistant response.
    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.push(LlmRole::Assistant, content.into());
    }

    /// Total turn count (not counting system prompt).
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }

    /// True if there are no user/assistant turns yet.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Build a flat prompt string suitable for a single-turn inference backend.
    ///
    /// Format: `<system>\n\n<user>: ...\n<assistant>: ...`
    pub fn build_prompt(&self) -> String {
        let mut parts = Vec::new();
        if !self.system_prompt.is_empty() {
            parts.push(format!("[System]: {}", self.system_prompt));
        }
        for turn in &self.turns {
            parts.push(format!("[{}]: {}", turn.role.as_str(), turn.content));
        }
        parts.push("[assistant]:".to_string());
        parts.join("\n")
    }

    /// Clear all turns but keep the system prompt.
    pub fn reset(&mut self) {
        self.turns.clear();
    }

    fn push(&mut self, role: LlmRole, content: String) {
        if self.turns.len() >= self.max_turns {
            self.turns.remove(0);
        }
        self.turns.push(LlmTurn { role, content, ts_ms: now_ms() });
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_is_empty() {
        let s = LlmSession::new("You are Jarvis.");
        assert!(s.is_empty());
        assert_eq!(s.turn_count(), 0);
    }

    #[test]
    fn push_user_increments_turns() {
        let mut s = LlmSession::new("sys");
        s.push_user("hello");
        assert_eq!(s.turn_count(), 1);
    }

    #[test]
    fn max_turns_evicts_oldest() {
        let mut s = LlmSession::new("sys").with_max_turns(3);
        s.push_user("a");
        s.push_assistant("b");
        s.push_user("c");
        s.push_assistant("d");  // evicts "a"
        assert_eq!(s.turn_count(), 3);
        assert_eq!(s.turns[0].content, "b");
    }

    #[test]
    fn build_prompt_contains_system_and_turns() {
        let mut s = LlmSession::new("Jarvis assistant");
        s.push_user("открой калькулятор");
        let prompt = s.build_prompt();
        assert!(prompt.contains("[System]:"));
        assert!(prompt.contains("открой калькулятор"));
        assert!(prompt.contains("[assistant]:"));
    }

    #[test]
    fn sessions_created_counter_increments() {
        let before = SESSIONS_CREATED.load(Ordering::Relaxed);
        let _s = LlmSession::new("test");
        assert!(SESSIONS_CREATED.load(Ordering::Relaxed) > before);
    }
}
