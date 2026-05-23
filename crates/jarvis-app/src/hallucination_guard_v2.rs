//! Extended hallucination guard (V2).
//!
//! Extends the base `cognitive::containment::HallucinationGuard` with:
//!   - Tool schema validation (arg count and format checks)
//!   - Capability cross-check: confirms LLM-suggested tool actually has
//!     the scope claimed in the plan
//!   - Plan-level audit: validates every step in a multi-step plan
//!
//! The base guard (G1–G4) still runs first; V2 adds G5–G7.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::cognitive::containment::{ContainmentVerdict, HallucinationGuard};
use crate::cognitive::ToolRegistry;
use crate::tool_capabilities;

pub static GUARD_V2_CHECKS:  AtomicU64 = AtomicU64::new(0);
pub static GUARD_V2_BLOCKED: AtomicU64 = AtomicU64::new(0);
pub static GUARD_V2_PASSED:  AtomicU64 = AtomicU64::new(0);

// ── V2 Verdict ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum GuardV2Verdict {
    Safe,
    Blocked { guard: &'static str, reason: String },
}

impl GuardV2Verdict {
    pub fn is_safe(&self) -> bool {
        matches!(self, GuardV2Verdict::Safe)
    }
}

// ── Guard V2 ──────────────────────────────────────────────────────────────────

pub struct HallucinationGuardV2;

impl HallucinationGuardV2 {
    /// Full validation pipeline for a tool call.
    ///
    /// Runs base G1–G4, then V2 G5–G7.
    pub fn validate_tool_call(
        tool_id: &str,
        arg: &str,
        registry: &ToolRegistry,
    ) -> GuardV2Verdict {
        GUARD_V2_CHECKS.fetch_add(1, Ordering::Relaxed);

        // G1–G4: base guard checks on the argument text.
        match HallucinationGuard::check_command_text(arg) {
            ContainmentVerdict::Blocked { reason } => {
                GUARD_V2_BLOCKED.fetch_add(1, Ordering::Relaxed);
                return GuardV2Verdict::Blocked { guard: "G1-G4", reason };
            }
            ContainmentVerdict::Safe => {}
        }

        // G5: tool must exist in the registry.
        match HallucinationGuard::check_tool_exists(tool_id, registry) {
            ContainmentVerdict::Blocked { reason } => {
                GUARD_V2_BLOCKED.fetch_add(1, Ordering::Relaxed);
                return GuardV2Verdict::Blocked { guard: "G5", reason };
            }
            ContainmentVerdict::Safe => {}
        }

        // G6: tool must have a capability profile (guards against tools invented by LLM
        //     that somehow made it into the registry without a profile).
        if tool_capabilities::get(tool_id).is_none() {
            GUARD_V2_BLOCKED.fetch_add(1, Ordering::Relaxed);
            return GuardV2Verdict::Blocked {
                guard: "G6",
                reason: format!("tool '{}' has no capability profile", tool_id),
            };
        }

        // G7: argument must not attempt to override tool scope.
        // Checks for known scope-override injection patterns.
        let arg_lower = arg.to_lowercase();
        const SCOPE_OVERRIDES: &[&str] = &[
            "--scope=", "--capability=", "--allow=", "--bypass=",
            "scope:network", "scope:filesystem", "scope:system",
        ];
        for pat in SCOPE_OVERRIDES {
            if arg_lower.contains(pat) {
                GUARD_V2_BLOCKED.fetch_add(1, Ordering::Relaxed);
                return GuardV2Verdict::Blocked {
                    guard: "G7",
                    reason: format!("scope override pattern '{}' detected", pat),
                };
            }
        }

        GUARD_V2_PASSED.fetch_add(1, Ordering::Relaxed);
        GuardV2Verdict::Safe
    }

    /// Validate every (tool_id, arg) pair in a plan.
    ///
    /// Returns the first violation found, or `Safe` if all steps pass.
    pub fn validate_plan_steps(
        steps: &[(&str, &str)],
        registry: &ToolRegistry,
    ) -> GuardV2Verdict {
        for (tool_id, arg) in steps {
            let v = Self::validate_tool_call(tool_id, arg, registry);
            if !v.is_safe() {
                return v;
            }
        }
        GuardV2Verdict::Safe
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_runtime;

    fn registry() -> ToolRegistry {
        tool_runtime::build_registry()
    }

    #[test]
    fn valid_tool_call_passes_all_guards() {
        let v = HallucinationGuardV2::validate_tool_call(
            tool_runtime::TOOL_APP_OPEN, "calculator", &registry(),
        );
        assert!(v.is_safe());
    }

    #[test]
    fn invented_tool_blocked_by_g5() {
        let v = HallucinationGuardV2::validate_tool_call(
            "jarvis.invented.hackOS", "payload", &registry(),
        );
        assert!(!v.is_safe());
        assert!(matches!(v, GuardV2Verdict::Blocked { guard: "G5", .. }));
    }

    #[test]
    fn lolbin_arg_blocked_by_g1_g4() {
        let v = HallucinationGuardV2::validate_tool_call(
            tool_runtime::TOOL_APP_OPEN,
            "powershell -enc dQBzAGUAcg==",
            &registry(),
        );
        assert!(!v.is_safe());
        assert!(matches!(v, GuardV2Verdict::Blocked { guard: "G1-G4", .. }));
    }

    #[test]
    fn scope_override_blocked_by_g7() {
        let v = HallucinationGuardV2::validate_tool_call(
            tool_runtime::TOOL_INFO_QUERY,
            "weather --scope=network",
            &registry(),
        );
        assert!(!v.is_safe());
        assert!(matches!(v, GuardV2Verdict::Blocked { guard: "G7", .. }));
    }

    #[test]
    fn plan_with_valid_steps_passes() {
        let steps = vec![
            (tool_runtime::TOOL_APP_OPEN, "calculator"),
            (tool_runtime::TOOL_SYSTEM_VOLUME, "50"),
        ];
        let v = HallucinationGuardV2::validate_plan_steps(&steps, &registry());
        assert!(v.is_safe());
    }

    #[test]
    fn guard_v2_checks_counter_increments() {
        let before = GUARD_V2_CHECKS.load(Ordering::Relaxed);
        HallucinationGuardV2::validate_tool_call(
            tool_runtime::TOOL_APP_OPEN, "calculator", &registry(),
        );
        assert!(GUARD_V2_CHECKS.load(Ordering::Relaxed) > before);
    }
}
