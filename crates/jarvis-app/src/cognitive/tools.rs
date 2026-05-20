#![allow(dead_code)]

use std::collections::HashMap;
use super::domains::Domain;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LatencyClass {
    Instant,    // < 100 ms
    Fast,       // < 1 s
    Slow,       // > 1 s
    Background, // non-blocking, requires observer
}

#[derive(Debug, Clone)]
pub struct ToolCapability {
    pub id: String,
    pub domain: Domain,
    pub description: String,
    pub requires_confirmation: bool,
    pub latency_class: LatencyClass,
    pub tags: Vec<String>,
}

pub struct ToolRegistry {
    tools: HashMap<String, ToolCapability>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, cap: ToolCapability) {
        self.tools.insert(cap.id.clone(), cap);
    }

    pub fn find_by_domain(&self, domain: &Domain) -> Vec<&ToolCapability> {
        self.tools.values().filter(|t| &t.domain == domain).collect()
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<&ToolCapability> {
        self.tools.values()
            .filter(|t| t.tags.iter().any(|tg| tg == tag))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&ToolCapability> {
        self.tools.get(id)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}
