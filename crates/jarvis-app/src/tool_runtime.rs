//! Concrete built-in tool definitions.
//!
//! Registers all first-party tools into a `ToolRouter` and `ToolRegistry`.
//! Tools are statically defined — no dynamic plugin loading.
//!
//! Tool IDs follow the domain.category.action convention.
//!
//! Safety contract: no tool executes here.  This module only describes
//! what tools exist and their risk/latency attributes.

use crate::cognitive::{
    Domain, LatencyClass, RetryPolicy, ToolCapability, ToolDescriptor, ToolRegistry, ToolRouter,
};
use crate::bus::RiskLevel;

// ── Built-in tool IDs ─────────────────────────────────────────────────────────

pub const TOOL_APP_OPEN:      &str = "app.open";
pub const TOOL_APP_CLOSE:     &str = "app.close";
pub const TOOL_SYSTEM_VOLUME: &str = "system.volume";
pub const TOOL_SYSTEM_MUTE:   &str = "system.mute";
pub const TOOL_REMINDER_SET:  &str = "reminder.set";
pub const TOOL_CLIPBOARD_READ:&str = "clipboard.read";
pub const TOOL_INFO_QUERY:    &str = "info.query";

// ── Registry builder ──────────────────────────────────────────────────────────

/// Build a `ToolRegistry` pre-populated with all first-party tools.
pub fn build_registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();

    reg.register(ToolCapability {
        id: TOOL_APP_OPEN.to_string(),
        domain: Domain::System,
        description: "Open a named application by its process name or display title.".to_string(),
        requires_confirmation: false,
        latency_class: LatencyClass::Fast,
        tags: vec!["app".into(), "launch".into()],
    });

    reg.register(ToolCapability {
        id: TOOL_APP_CLOSE.to_string(),
        domain: Domain::System,
        description: "Close a named application gracefully.".to_string(),
        requires_confirmation: false,
        latency_class: LatencyClass::Fast,
        tags: vec!["app".into(), "close".into()],
    });

    reg.register(ToolCapability {
        id: TOOL_SYSTEM_VOLUME.to_string(),
        domain: Domain::System,
        description: "Set system audio volume (0–100).".to_string(),
        requires_confirmation: false,
        latency_class: LatencyClass::Instant,
        tags: vec!["audio".into(), "system".into()],
    });

    reg.register(ToolCapability {
        id: TOOL_SYSTEM_MUTE.to_string(),
        domain: Domain::System,
        description: "Toggle or set system mute state.".to_string(),
        requires_confirmation: false,
        latency_class: LatencyClass::Instant,
        tags: vec!["audio".into(), "system".into()],
    });

    reg.register(ToolCapability {
        id: TOOL_REMINDER_SET.to_string(),
        domain: Domain::Productivity,
        description: "Create a timed reminder with a message.".to_string(),
        requires_confirmation: false,
        latency_class: LatencyClass::Instant,
        tags: vec!["reminder".into(), "schedule".into()],
    });

    reg.register(ToolCapability {
        id: TOOL_CLIPBOARD_READ.to_string(),
        domain: Domain::System,
        description: "Read the current clipboard text content.".to_string(),
        requires_confirmation: false,
        latency_class: LatencyClass::Instant,
        tags: vec!["clipboard".into()],
    });

    reg.register(ToolCapability {
        id: TOOL_INFO_QUERY.to_string(),
        domain: Domain::Knowledge,
        description: "Answer a factual query using local knowledge.".to_string(),
        requires_confirmation: false,
        latency_class: LatencyClass::Slow,
        tags: vec!["query".into(), "knowledge".into()],
    });

    reg
}

// ── Router builder ────────────────────────────────────────────────────────────

/// Build a `ToolRouter` with all first-party descriptors registered.
pub fn build_router() -> ToolRouter {
    let mut router = ToolRouter::new();

    router.register(ToolDescriptor {
        id:                   TOOL_APP_OPEN.to_string(),
        description:          "Open a named application.".to_string(),
        risk_level:           RiskLevel::Low,
        retry_policy:         RetryPolicy::Linear { max: 1, delay_ms: 200 },
        timeout_ms:           3_000,
        deterministic:        false,
        requires_confirmation: false,
        latency_budget_ms:    1_000,
    });

    router.register(ToolDescriptor {
        id:                   TOOL_APP_CLOSE.to_string(),
        description:          "Close a named application.".to_string(),
        risk_level:           RiskLevel::Medium,
        retry_policy:         RetryPolicy::Never,
        timeout_ms:           3_000,
        deterministic:        false,
        requires_confirmation: false,
        latency_budget_ms:    1_000,
    });

    router.register(ToolDescriptor {
        id:                   TOOL_SYSTEM_VOLUME.to_string(),
        description:          "Set system volume.".to_string(),
        risk_level:           RiskLevel::Low,
        retry_policy:         RetryPolicy::Linear { max: 2, delay_ms: 100 },
        timeout_ms:           500,
        deterministic:        true,
        requires_confirmation: false,
        latency_budget_ms:    200,
    });

    router.register(ToolDescriptor {
        id:                   TOOL_SYSTEM_MUTE.to_string(),
        description:          "Toggle system mute.".to_string(),
        risk_level:           RiskLevel::Low,
        retry_policy:         RetryPolicy::Linear { max: 2, delay_ms: 100 },
        timeout_ms:           500,
        deterministic:        true,
        requires_confirmation: false,
        latency_budget_ms:    200,
    });

    router.register(ToolDescriptor {
        id:                   TOOL_REMINDER_SET.to_string(),
        description:          "Create a reminder.".to_string(),
        risk_level:           RiskLevel::Low,
        retry_policy:         RetryPolicy::Linear { max: 2, delay_ms: 100 },
        timeout_ms:           500,
        deterministic:        true,
        requires_confirmation: false,
        latency_budget_ms:    200,
    });

    router.register(ToolDescriptor {
        id:                   TOOL_CLIPBOARD_READ.to_string(),
        description:          "Read clipboard content.".to_string(),
        risk_level:           RiskLevel::Low,
        retry_policy:         RetryPolicy::Never,
        timeout_ms:           200,
        deterministic:        false,
        requires_confirmation: false,
        latency_budget_ms:    100,
    });

    router.register(ToolDescriptor {
        id:                   TOOL_INFO_QUERY.to_string(),
        description:          "Answer a factual query.".to_string(),
        risk_level:           RiskLevel::Low,
        retry_policy:         RetryPolicy::Linear { max: 1, delay_ms: 500 },
        timeout_ms:           15_000,
        deterministic:        false,
        requires_confirmation: false,
        latency_budget_ms:    5_000,
    });

    router
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognitive::ToolRouteDecision;

    #[test]
    fn registry_has_all_builtin_tools() {
        let reg = build_registry();
        assert_eq!(reg.len(), 7);
    }

    #[test]
    fn router_allows_low_risk_tool() {
        let router = build_router();
        let decision = router.route(TOOL_APP_OPEN);
        assert!(matches!(decision, ToolRouteDecision::Allow(_)));
    }

    #[test]
    fn router_not_found_for_unknown_tool() {
        let router = build_router();
        let decision = router.route("jarvis.hack.system");
        assert!(matches!(decision, ToolRouteDecision::NotFound { .. }));
    }

    #[test]
    fn registry_find_by_tag_app() {
        let reg = build_registry();
        let hits = reg.find_by_tag("app");
        assert!(!hits.is_empty());
    }

    #[test]
    fn all_tools_registered_in_both() {
        let reg = build_registry();
        let router = build_router();
        for id in &[
            TOOL_APP_OPEN, TOOL_APP_CLOSE, TOOL_SYSTEM_VOLUME,
            TOOL_SYSTEM_MUTE, TOOL_REMINDER_SET, TOOL_CLIPBOARD_READ,
            TOOL_INFO_QUERY,
        ] {
            assert!(reg.get(id).is_some(), "missing in registry: {}", id);
            assert!(router.get(id).is_some(), "missing in router: {}", id);
        }
    }
}
