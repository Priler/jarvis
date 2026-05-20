#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use super::domains::{Domain, classify_domain};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedIntent {
    pub raw_text: String,
    pub normalized_text: String,
    pub domain: Domain,
    pub entities: Vec<Entity>,
    pub urgency: Urgency,
    pub context_dependent: bool,
    pub matched_intent_id: Option<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub kind: EntityKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Number,
    Percentage,
    Duration,
    Url,
    SearchQuery,
    AppName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Urgency {
    High,
    #[default]
    Normal,
    Low,
}

impl EnrichedIntent {
    pub fn from_text(text: &str) -> Self {
        let normalized = text.to_lowercase().trim().to_string();
        let domain = classify_domain(&normalized);
        let entities = extract_entities(&normalized);
        let urgency = detect_urgency(&normalized);
        let context_dependent = detect_context_dependency(&normalized);

        Self {
            raw_text: text.to_string(),
            normalized_text: normalized,
            domain,
            entities,
            urgency,
            context_dependent,
            matched_intent_id: None,
            confidence: 0.0,
        }
    }

    pub fn with_intent(mut self, intent_id: String, confidence: f64) -> Self {
        self.matched_intent_id = Some(intent_id);
        self.confidence = confidence;
        self
    }
}

fn extract_entities(text: &str) -> Vec<Entity> {
    let mut entities = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();

    for (i, word) in words.iter().enumerate() {
        let clean = word.trim_end_matches('%');
        if let Ok(_) = clean.parse::<f64>() {
            let kind = if word.ends_with('%') {
                EntityKind::Percentage
            } else {
                EntityKind::Number
            };
            entities.push(Entity { kind, value: (*word).to_string() });
        }
        // Duration: "<num> <unit>"
        if matches!(*word, "minute" | "minutes" | "hour" | "hours" | "second" | "seconds") {
            if i > 0 {
                if words[i - 1].parse::<f64>().is_ok() {
                    entities.push(Entity {
                        kind: EntityKind::Duration,
                        value: format!("{} {}", words[i - 1], word),
                    });
                }
            }
        }
    }

    for word in &words {
        if word.starts_with("http://") || word.starts_with("https://") || word.starts_with("www.") {
            entities.push(Entity { kind: EntityKind::Url, value: (*word).to_string() });
        }
    }

    entities
}

fn detect_urgency(text: &str) -> Urgency {
    const HIGH: &[&str] = &["quickly", "now", "immediately", "urgent", "fast", "right now", "asap"];
    const LOW: &[&str] = &["whenever", "sometime", "later", "eventually", "no rush"];

    if HIGH.iter().any(|kw| text.contains(kw)) {
        return Urgency::High;
    }
    if LOW.iter().any(|kw| text.contains(kw)) {
        return Urgency::Low;
    }
    Urgency::Normal
}

fn detect_context_dependency(text: &str) -> bool {
    const CTX: &[&str] = &["it", "that", "this", "there", "the other", "again", "also", "after that", "then"];
    CTX.iter().any(|kw| text.contains(kw))
}
