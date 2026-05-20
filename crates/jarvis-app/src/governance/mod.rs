#![allow(dead_code)]

use crate::bus::RiskLevel;

// ── Decision ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GovernanceDecision {
    pub allowed: bool,
    pub risk_level: RiskLevel,
    pub reason: String,
    pub requires_confirmation: bool,
}

impl GovernanceDecision {
    pub fn allow(risk_level: RiskLevel) -> Self {
        Self { allowed: true, risk_level, reason: String::new(), requires_confirmation: false }
    }

    pub fn deny(reason: impl Into<String>, risk_level: RiskLevel) -> Self {
        Self { allowed: false, risk_level, reason: reason.into(), requires_confirmation: false }
    }

    pub fn confirm_required(reason: impl Into<String>, risk_level: RiskLevel) -> Self {
        Self { allowed: true, risk_level, reason: reason.into(), requires_confirmation: true }
    }
}

// ── Capability set ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    pub filesystem_write: bool,
    pub network_allowed: bool,
    pub shell_execution: bool,
    pub system_modification: bool,
    pub automation_allowed: bool,
    pub allowed_paths: Vec<String>,
}

// ── Governance layer ──────────────────────────────────────────────────────────

pub struct GovernanceLayer {
    /// When true, medium-risk actions also require confirmation.
    pub strict_mode: bool,
}

impl GovernanceLayer {
    pub fn new() -> Self {
        Self { strict_mode: false }
    }

    /// Check whether a command is permitted to execute.
    pub fn check_command(
        &self,
        cmd_type: &str,
        cli_cmd: &str,
        sandbox: &str,
    ) -> GovernanceDecision {
        let risk = self.classify_command_risk(cmd_type, cli_cmd, sandbox);

        match risk {
            RiskLevel::Critical => GovernanceDecision::deny(
                format!("Critical-risk action blocked by governance: '{}'", cli_cmd),
                RiskLevel::Critical,
            ),
            RiskLevel::High => GovernanceDecision::confirm_required(
                format!("High-risk action requires confirmation: '{}'", cli_cmd),
                RiskLevel::High,
            ),
            RiskLevel::Medium if self.strict_mode => GovernanceDecision::confirm_required(
                "Strict mode: medium-risk action requires confirmation".to_string(),
                RiskLevel::Medium,
            ),
            other => GovernanceDecision::allow(other),
        }
    }

    /// Classify the risk level of a CLI/AHK/Lua command.
    pub fn classify_command_risk(
        &self,
        cmd_type: &str,
        cli_cmd: &str,
        sandbox: &str,
    ) -> RiskLevel {
        // Lua with full sandbox = arbitrary code execution.
        if cmd_type == "lua" && sandbox == "full" {
            return RiskLevel::High;
        }

        let c = cli_cmd.to_lowercase();

        const CRITICAL: &[&str] = &[
            "format ", "diskpart", "cipher /w", "del /f /s /q",
            "rmdir /s", "rd /s",
        ];
        if CRITICAL.iter().any(|pat| c.contains(pat)) {
            return RiskLevel::Critical;
        }

        const HIGH: &[&str] = &[
            "shutdown", "restart-computer", "reg delete", "reg add",
            "net user", "net localgroup", "takeown", "icacls",
            "powershell -", "cmd /c", "wscript", "cscript", "mshta",
            "rundll32", "regsvr32",
        ];
        if HIGH.iter().any(|pat| c.starts_with(pat) || c.contains(pat)) {
            return RiskLevel::High;
        }

        const MEDIUM: &[&str] = &["del ", "rmdir", "remove-item", "move ", "ren ", "sc stop", "taskkill"];
        if MEDIUM.iter().any(|pat| c.starts_with(pat)) {
            return RiskLevel::Medium;
        }

        RiskLevel::Low
    }

    /// Quick permit check for workflow steps (registered workflows have slightly
    /// higher implicit trust than raw voice commands).
    pub fn check_workflow_step(&self, description: &str) -> GovernanceDecision {
        let d = description.to_lowercase();
        let risk = if d.contains("delete") || d.contains("format") || d.contains("shutdown") {
            RiskLevel::High
        } else if d.contains("install") || d.contains("uninstall") {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };
        GovernanceDecision::allow(risk)
    }
}
