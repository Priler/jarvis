#![allow(dead_code)]

use std::collections::HashMap;
use super::domains::Domain;
use crate::bus::RiskLevel;

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

// ── Tool descriptor (extended metadata for governance + planner) ──────────────

/// Retry semantics for failed tool invocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryPolicy {
    /// Never retry; failure is propagated immediately.
    Never,
    /// Retry up to `max` times with a fixed `delay_ms` between attempts.
    Linear { max: u8, delay_ms: u64 },
}

/// Extended tool metadata used by the planner and governance layer.
///
/// `ToolDescriptor` is richer than `ToolCapability` and carries the
/// risk/safety attributes that the executor needs to enforce.
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub id: String,
    pub description: String,
    pub risk_level: RiskLevel,
    pub retry_policy: RetryPolicy,
    pub timeout_ms: u64,
    /// True if the tool produces the same output for the same input.
    pub deterministic: bool,
    /// If true, the governance layer must confirm before execution.
    pub requires_confirmation: bool,
    /// Soft latency target; violations are logged but not blocked.
    pub latency_budget_ms: u64,
}

impl ToolDescriptor {
    pub fn low_risk(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            risk_level: RiskLevel::Low,
            retry_policy: RetryPolicy::Linear { max: 2, delay_ms: 100 },
            timeout_ms: 1000,
            deterministic: true,
            requires_confirmation: false,
            latency_budget_ms: 500,
        }
    }

    pub fn high_risk(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            risk_level: RiskLevel::High,
            retry_policy: RetryPolicy::Never,
            timeout_ms: 5000,
            deterministic: false,
            requires_confirmation: true,
            latency_budget_ms: 2000,
        }
    }
}

// ── Tool router ───────────────────────────────────────────────────────────────

/// Routes a resolved tool invocation through risk checks before handing off
/// to the execution layer.
///
/// The router does NOT execute — it returns a `ToolRouteDecision` that the
/// executor interprets. The LLM never receives a `ToolRouteDecision`.
#[derive(Debug)]
pub enum ToolRouteDecision {
    /// Proceed: descriptor validated, risk accepted.
    Allow(ToolDescriptor),
    /// Pause: high-risk tool, must be confirmed before proceeding.
    RequireConfirmation(ToolDescriptor),
    /// Hard block: tool is Blocked-tier — no confirmation path.
    Block { reason: String },
    /// Tool ID not registered.
    NotFound { id: String },
}

pub struct ToolRouter {
    descriptors: HashMap<String, ToolDescriptor>,
}

impl ToolRouter {
    pub fn new() -> Self {
        Self { descriptors: HashMap::new() }
    }

    pub fn register(&mut self, d: ToolDescriptor) {
        self.descriptors.insert(d.id.clone(), d);
    }

    pub fn route(&self, tool_id: &str) -> ToolRouteDecision {
        match self.descriptors.get(tool_id) {
            None => ToolRouteDecision::NotFound { id: tool_id.to_string() },
            Some(d) => match d.risk_level {
                RiskLevel::Blocked | RiskLevel::Critical => ToolRouteDecision::Block {
                    reason: format!("tool '{}' is risk={}", tool_id, d.risk_level.as_str()),
                },
                RiskLevel::High if d.requires_confirmation => {
                    ToolRouteDecision::RequireConfirmation(d.clone())
                }
                _ => ToolRouteDecision::Allow(d.clone()),
            },
        }
    }

    pub fn get(&self, id: &str) -> Option<&ToolDescriptor> {
        self.descriptors.get(id)
    }
}
