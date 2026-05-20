#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use super::domains::Domain;

const WORKING_MEMORY_CAPACITY: usize = 20;
const EPISODIC_MAX_RECORDS: usize = 500;

// ── Conversation turn ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub text: String,
    pub domain: Domain,
    pub intent_id: Option<String>,
    pub entities: Vec<String>,
    pub success: bool,
    pub timestamp_ms: u64,
}

// ── Working memory ────────────────────────────────────────────────────────────

pub struct WorkingMemory {
    turns: VecDeque<ConversationTurn>,
    capacity: usize,
    pub active_domain: Option<Domain>,
    pub last_entities: Vec<String>,
    pub pending_clarification: Option<String>,
    pub conversation_depth: u32,
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self {
            turns: VecDeque::new(),
            capacity: WORKING_MEMORY_CAPACITY,
            active_domain: None,
            last_entities: Vec::new(),
            pending_clarification: None,
            conversation_depth: 0,
        }
    }

    pub fn push(&mut self, turn: ConversationTurn) {
        self.active_domain = Some(turn.domain.clone());
        self.last_entities = turn.entities.clone();
        self.conversation_depth = self.conversation_depth.saturating_add(1);
        if self.turns.len() >= self.capacity {
            self.turns.pop_front();
        }
        self.turns.push_back(turn);
    }

    pub fn recent_turns(&self, n: usize) -> Vec<&ConversationTurn> {
        self.turns.iter().rev().take(n).collect()
    }

    pub fn last_successful(&self) -> Option<&ConversationTurn> {
        self.turns.iter().rev().find(|t| t.success)
    }

    /// If text contains a contextual pronoun, return a summary of what it likely refers to.
    pub fn find_context_for(&self, text: &str) -> Option<String> {
        const CTX_WORDS: &[&str] = &[" it", " that", " this", " there"];
        let t = text.to_lowercase();
        let has_ref = CTX_WORDS.iter().any(|kw| t.contains(kw));
        if !has_ref {
            return None;
        }
        if let Some(last) = self.last_successful() {
            if !last.entities.is_empty() {
                return Some(format!(
                    "refers to '{}' (last {} command)",
                    last.entities.join(", "),
                    last.domain.as_str()
                ));
            }
            return Some(format!("last {} command", last.domain.as_str()));
        }
        None
    }

    pub fn reset_session(&mut self) {
        self.pending_clarification = None;
        self.conversation_depth = 0;
    }
}

// ── Long-term memory ──────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LongTermMemory {
    pub episodic: Vec<EpisodicRecord>,
    pub preferences: PreferenceMap,
    pub procedural: Vec<ProceduralRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicRecord {
    pub timestamp_ms: u64,
    pub domain: String,
    pub text: String,
    pub intent_id: Option<String>,
    pub success: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PreferenceMap {
    pub domain_usage: HashMap<String, u32>,
    pub intent_usage: HashMap<String, u32>,
    pub last_used_ms: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralRecord {
    pub id: String,
    pub description: String,
    pub steps: Vec<String>,
    pub success_count: u32,
}

impl LongTermMemory {
    pub fn load(path: &PathBuf) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = std::fs::write(path, &content) {
                    warn!("[COGNITIVE] Failed to save long-term memory: {}", e);
                }
            }
            Err(e) => warn!("[COGNITIVE] Failed to serialize long-term memory: {}", e),
        }
    }

    pub fn record_episodic(&mut self, turn: &ConversationTurn) {
        self.episodic.push(EpisodicRecord {
            timestamp_ms: turn.timestamp_ms,
            domain: turn.domain.as_str().to_string(),
            text: turn.text.clone(),
            intent_id: turn.intent_id.clone(),
            success: turn.success,
        });
        if self.episodic.len() > EPISODIC_MAX_RECORDS {
            let drain_to = self.episodic.len() - EPISODIC_MAX_RECORDS;
            self.episodic.drain(..drain_to);
        }
    }

    pub fn update_preference(&mut self, domain: &str, intent_id: Option<&str>, timestamp_ms: u64) {
        *self.preferences.domain_usage.entry(domain.to_string()).or_insert(0) += 1;
        if let Some(id) = intent_id {
            *self.preferences.intent_usage.entry(id.to_string()).or_insert(0) += 1;
            self.preferences.last_used_ms.insert(id.to_string(), timestamp_ms);
        }
    }

    pub fn preferred_domain(&self) -> Option<&str> {
        self.preferences.domain_usage.iter()
            .max_by_key(|(_, v)| *v)
            .map(|(k, _)| k.as_str())
    }

    /// Return episodic records with any keyword overlap with `text`, best-match first.
    pub fn recall_similar(&self, text: &str, limit: usize) -> Vec<&EpisodicRecord> {
        let t = text.to_lowercase();
        let words: Vec<&str> = t.split_whitespace().collect();
        let mut scored: Vec<(usize, &EpisodicRecord)> = self.episodic.iter()
            .filter_map(|r| {
                let score = words.iter()
                    .filter(|w| r.text.to_lowercase().contains(*w))
                    .count();
                if score > 0 { Some((score, r)) } else { None }
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().take(limit).map(|(_, r)| r).collect()
    }
}
