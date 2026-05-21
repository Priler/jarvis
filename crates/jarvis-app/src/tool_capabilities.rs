//! Extended tool capability model.
//!
//! `ToolCapabilityProfile` enriches the Phase 12 `ToolDescriptor` with:
//!   - `ToolScope` — what system resources the tool accesses
//!   - `SideEffect` — what state the tool changes
//!   - Confirmation rules and rollback availability
//!
//! Used by the sandbox, planner_v2, and execution_verifier.

use std::collections::HashMap;
use once_cell::sync::Lazy;

// ── Scope ─────────────────────────────────────────────────────────────────────

/// What system resources the tool is allowed to access.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ToolScope {
    /// No external resource access (pure computation).
    None,
    /// Launches or closes UI applications.
    UserInterface,
    /// Reads from or writes to the filesystem.
    Filesystem,
    /// Modifies system-level settings (volume, display, etc.).
    SystemSettings,
    /// Accesses the system clipboard.
    Clipboard,
    /// Schedules events or timers.
    Scheduler,
    /// Accesses local knowledge base or query engine.
    LocalKnowledge,
}

// ── SideEffect ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SideEffect {
    /// No side effects — safe to retry.
    None,
    /// Creates a new OS process.
    CreatesProcess,
    /// Terminates a running OS process.
    TerminatesProcess,
    /// Modifies system audio state.
    ChangesAudioState,
    /// Modifies clipboard content.
    ModifiesClipboard,
    /// Creates or modifies a calendar/reminder entry.
    CreatesReminder,
    /// Queries local data — read-only.
    ReadOnly,
}

impl SideEffect {
    pub fn is_reversible(&self) -> bool {
        matches!(self, SideEffect::None | SideEffect::ReadOnly | SideEffect::ChangesAudioState)
    }
}

// ── Capability profile ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCapabilityProfile {
    pub tool_id:              String,
    /// Risk level as string ("low", "medium", "high", "critical").
    pub risk_level:           String,
    pub scope:                ToolScope,
    pub side_effect:          SideEffect,
    /// True when action cannot be undone (process kill, destructive write).
    pub irreversible:         bool,
    /// True when the governance layer must confirm before execution.
    pub requires_confirmation: bool,
    /// True if this tool can be safely retried on failure.
    pub retriable:            bool,
    /// Brief human-readable description of what confirmation should say.
    pub confirmation_prompt:  Option<String>,
}

impl ToolCapabilityProfile {
    pub fn is_safe_to_auto_execute(&self) -> bool {
        !self.requires_confirmation
            && self.risk_level == "low"
            && !self.irreversible
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

static PROFILES: Lazy<HashMap<String, ToolCapabilityProfile>> = Lazy::new(build_profiles);

pub fn get(tool_id: &str) -> Option<&'static ToolCapabilityProfile> {
    PROFILES.get(tool_id)
}

pub fn all() -> impl Iterator<Item = &'static ToolCapabilityProfile> {
    PROFILES.values()
}

pub fn count() -> usize {
    PROFILES.len()
}

fn build_profiles() -> HashMap<String, ToolCapabilityProfile> {
    use crate::tool_runtime::*;
    let mut m = HashMap::new();

    m.insert(TOOL_APP_OPEN.to_string(), ToolCapabilityProfile {
        tool_id:              TOOL_APP_OPEN.to_string(),
        risk_level:           "low".to_string(),
        scope:                ToolScope::UserInterface,
        side_effect:          SideEffect::CreatesProcess,
        irreversible:         false,
        requires_confirmation: false,
        retriable:            true,
        confirmation_prompt:  None,
    });

    m.insert(TOOL_APP_CLOSE.to_string(), ToolCapabilityProfile {
        tool_id:              TOOL_APP_CLOSE.to_string(),
        risk_level:           "medium".to_string(),
        scope:                ToolScope::UserInterface,
        side_effect:          SideEffect::TerminatesProcess,
        irreversible:         false,
        requires_confirmation: false,
        retriable:            false,
        confirmation_prompt:  None,
    });

    m.insert(TOOL_SYSTEM_VOLUME.to_string(), ToolCapabilityProfile {
        tool_id:              TOOL_SYSTEM_VOLUME.to_string(),
        risk_level:           "low".to_string(),
        scope:                ToolScope::SystemSettings,
        side_effect:          SideEffect::ChangesAudioState,
        irreversible:         false,
        requires_confirmation: false,
        retriable:            true,
        confirmation_prompt:  None,
    });

    m.insert(TOOL_SYSTEM_MUTE.to_string(), ToolCapabilityProfile {
        tool_id:              TOOL_SYSTEM_MUTE.to_string(),
        risk_level:           "low".to_string(),
        scope:                ToolScope::SystemSettings,
        side_effect:          SideEffect::ChangesAudioState,
        irreversible:         false,
        requires_confirmation: false,
        retriable:            true,
        confirmation_prompt:  None,
    });

    m.insert(TOOL_REMINDER_SET.to_string(), ToolCapabilityProfile {
        tool_id:              TOOL_REMINDER_SET.to_string(),
        risk_level:           "low".to_string(),
        scope:                ToolScope::Scheduler,
        side_effect:          SideEffect::CreatesReminder,
        irreversible:         false,
        requires_confirmation: false,
        retriable:            true,
        confirmation_prompt:  None,
    });

    m.insert(TOOL_CLIPBOARD_READ.to_string(), ToolCapabilityProfile {
        tool_id:              TOOL_CLIPBOARD_READ.to_string(),
        risk_level:           "low".to_string(),
        scope:                ToolScope::Clipboard,
        side_effect:          SideEffect::ReadOnly,
        irreversible:         false,
        requires_confirmation: false,
        retriable:            true,
        confirmation_prompt:  None,
    });

    m.insert(TOOL_INFO_QUERY.to_string(), ToolCapabilityProfile {
        tool_id:              TOOL_INFO_QUERY.to_string(),
        risk_level:           "low".to_string(),
        scope:                ToolScope::LocalKnowledge,
        side_effect:          SideEffect::ReadOnly,
        irreversible:         false,
        requires_confirmation: false,
        retriable:            true,
        confirmation_prompt:  None,
    });

    m
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_tools_have_profiles() {
        assert_eq!(count(), 7);
    }

    #[test]
    fn app_open_is_safe_to_auto_execute() {
        let p = get(crate::tool_runtime::TOOL_APP_OPEN).unwrap();
        assert!(p.is_safe_to_auto_execute());
    }

    #[test]
    fn app_close_is_not_safe_to_auto_execute() {
        let p = get(crate::tool_runtime::TOOL_APP_CLOSE).unwrap();
        // Medium risk — not auto-safe
        assert!(!p.is_safe_to_auto_execute());
    }

    #[test]
    fn clipboard_read_is_readonly() {
        let p = get(crate::tool_runtime::TOOL_CLIPBOARD_READ).unwrap();
        assert_eq!(p.side_effect, SideEffect::ReadOnly);
        assert!(p.side_effect.is_reversible());
    }

    #[test]
    fn unknown_tool_returns_none() {
        assert!(get("jarvis.invented.tool").is_none());
    }
}
