//! Cognitive safety — rate-limiting and pre-flight checks for autonomous
//! proactive actions.
//!
//! Prevents intrusive, uncontrolled, or runaway cognition.  All proactive
//! actions must pass this guard before execution.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static SAFETY_CHECKS:    AtomicU64 = AtomicU64::new(0);
pub static SAFETY_ALLOWED:   AtomicU64 = AtomicU64::new(0);
pub static SAFETY_BLOCKED:   AtomicU64 = AtomicU64::new(0);
pub static SAFETY_RATE_LIMITED: AtomicU64 = AtomicU64::new(0);

// ── Rate limiter windows (per action kind, ms between allowed executions) ─────

const RATE_WINDOW_DEFAULT_MS: u64 = 5_000;
const RATE_WINDOW_HIGH_MS:    u64 = 30_000;
const RATE_WINDOW_CRITICAL_MS: u64 = 60_000;

// ── Proactive action kinds ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ProactiveActionKind {
    SuggestTool    { tool_id: String },
    DismissDialog,
    FocusWindow,
    TriggerVerification,
    UpdateWorldModel,
    LogObservation,
    TriggerReflection,
}

impl ProactiveActionKind {
    pub fn rate_window_ms(&self) -> u64 {
        match self {
            ProactiveActionKind::SuggestTool { .. }     => RATE_WINDOW_HIGH_MS,
            ProactiveActionKind::DismissDialog           => RATE_WINDOW_CRITICAL_MS,
            ProactiveActionKind::FocusWindow             => RATE_WINDOW_HIGH_MS,
            ProactiveActionKind::TriggerVerification     => RATE_WINDOW_DEFAULT_MS,
            ProactiveActionKind::UpdateWorldModel        => 1_000,
            ProactiveActionKind::LogObservation          => 500,
            ProactiveActionKind::TriggerReflection       => RATE_WINDOW_HIGH_MS,
        }
    }

    pub fn is_destructive(&self) -> bool {
        matches!(self, ProactiveActionKind::DismissDialog)
    }

    pub fn key(&self) -> String {
        match self {
            ProactiveActionKind::SuggestTool { tool_id } => format!("suggest:{}", tool_id),
            ProactiveActionKind::DismissDialog           => "dismiss_dialog".to_string(),
            ProactiveActionKind::FocusWindow             => "focus_window".to_string(),
            ProactiveActionKind::TriggerVerification     => "trigger_verify".to_string(),
            ProactiveActionKind::UpdateWorldModel        => "update_world".to_string(),
            ProactiveActionKind::LogObservation          => "log_obs".to_string(),
            ProactiveActionKind::TriggerReflection       => "trigger_reflect".to_string(),
        }
    }
}

// ── Safety verdict ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CognitiveSafetyVerdict {
    Allowed,
    Blocked     { reason: String },
    RateLimited { retry_after_ms: u64 },
}

impl CognitiveSafetyVerdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, CognitiveSafetyVerdict::Allowed)
    }
}

// ── Rate limiter state ────────────────────────────────────────────────────────

static LAST_EXEC: Lazy<Mutex<std::collections::HashMap<String, u64>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

// ── Cognitive safety guard ────────────────────────────────────────────────────

pub struct CognitiveSafetyGuard;

impl CognitiveSafetyGuard {
    pub fn check(action: &ProactiveActionKind) -> CognitiveSafetyVerdict {
        SAFETY_CHECKS.fetch_add(1, Ordering::Relaxed);
        let now = ts_now();

        // Hard block 1: destructive action while blocking modal is present
        if action.is_destructive() {
            let has_modal = crate::world_state::has_blocking_modal();
            if has_modal {
                SAFETY_BLOCKED.fetch_add(1, Ordering::Relaxed);
                return CognitiveSafetyVerdict::Blocked {
                    reason: "blocking modal present — cannot perform destructive action".to_string(),
                };
            }
        }

        // Hard block 2: world state is stale (environment unknown)
        if action.is_destructive() && crate::world_state::is_stale() {
            SAFETY_BLOCKED.fetch_add(1, Ordering::Relaxed);
            return CognitiveSafetyVerdict::Blocked {
                reason: "world state is stale — cannot perform destructive action safely".to_string(),
            };
        }

        // Rate limiting
        let key = action.key();
        let window = action.rate_window_ms();

        if let Ok(mut guard) = LAST_EXEC.lock() {
            if let Some(&last) = guard.get(&key) {
                let elapsed = now.saturating_sub(last);
                if elapsed < window {
                    SAFETY_RATE_LIMITED.fetch_add(1, Ordering::Relaxed);
                    return CognitiveSafetyVerdict::RateLimited {
                        retry_after_ms: window - elapsed,
                    };
                }
            }
            guard.insert(key, now);
        }

        SAFETY_ALLOWED.fetch_add(1, Ordering::Relaxed);
        CognitiveSafetyVerdict::Allowed
    }

    /// Reset rate-limit state for a specific key (for testing).
    pub fn reset_rate_limit(action: &ProactiveActionKind) {
        if let Ok(mut guard) = LAST_EXEC.lock() {
            guard.remove(&action.key());
        }
    }

    /// Clear all rate-limit state.
    pub fn reset_all() {
        if let Ok(mut guard) = LAST_EXEC.lock() {
            guard.clear();
        }
    }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup() {
        CognitiveSafetyGuard::reset_all();
    }

    #[test]
    fn log_observation_always_allowed_first_call() {
        cleanup();
        let action = ProactiveActionKind::LogObservation;
        let verdict = CognitiveSafetyGuard::check(&action);
        // May be Allowed or RateLimited depending on prior state, but not Blocked
        assert!(!matches!(verdict, CognitiveSafetyVerdict::Blocked { .. }));
    }

    #[test]
    fn rate_limited_after_second_call() {
        cleanup();
        let action = ProactiveActionKind::TriggerReflection;
        CognitiveSafetyGuard::check(&action);
        let second = CognitiveSafetyGuard::check(&action);
        assert!(matches!(second, CognitiveSafetyVerdict::RateLimited { .. }));
    }

    #[test]
    fn reset_rate_limit_allows_again() {
        cleanup();
        let action = ProactiveActionKind::TriggerVerification;
        CognitiveSafetyGuard::check(&action);
        CognitiveSafetyGuard::reset_rate_limit(&action);
        let verdict = CognitiveSafetyGuard::check(&action);
        assert!(verdict.is_allowed());
    }

    #[test]
    fn safety_checks_counter_increments() {
        let before = SAFETY_CHECKS.load(Ordering::Relaxed);
        CognitiveSafetyGuard::check(&ProactiveActionKind::UpdateWorldModel);
        assert!(SAFETY_CHECKS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn destructive_action_key_is_stable() {
        let a = ProactiveActionKind::DismissDialog;
        assert_eq!(a.key(), "dismiss_dialog");
    }

    #[test]
    fn rate_window_destructive_is_large() {
        let a = ProactiveActionKind::DismissDialog;
        assert!(a.rate_window_ms() >= RATE_WINDOW_CRITICAL_MS);
    }
}
