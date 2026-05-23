#![allow(dead_code)]

//! Failure classification engine.
//!
//! Provides a unified taxonomy for all runtime failures so watchdog incidents,
//! recovery actions, and test harness assertions all use consistent terminology.

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FailureKind {
    Audio,
    Ipc,
    Wake,
    Stt,
    Lifecycle,
    Timing,
    StateCorruption,
    Replay,
    CommandExec,
    LuaSandbox,
}

impl FailureKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Ipc => "ipc",
            Self::Wake => "wake",
            Self::Stt => "stt",
            Self::Lifecycle => "lifecycle",
            Self::Timing => "timing",
            Self::StateCorruption => "state_corruption",
            Self::Replay => "replay",
            Self::CommandExec => "command_exec",
            Self::LuaSandbox => "lua_sandbox",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum FailureSeverity {
    Info = 0,
    Warning = 1,
    Error = 2,
    Critical = 3,
}

impl FailureSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

pub struct FailureRecord {
    pub kind: FailureKind,
    pub severity: FailureSeverity,
    pub subsystem: &'static str,
    pub detail: String,
    pub ts_ms: u64,
}

impl FailureRecord {
    pub fn new(
        kind: FailureKind,
        severity: FailureSeverity,
        subsystem: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            severity,
            subsystem,
            detail: detail.into(),
            ts_ms: now_ms(),
        }
    }

    pub fn to_json_line(&self) -> String {
        let d = self.detail.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
            "{{\"ts\":{},\"kind\":\"{}\",\"severity\":\"{}\",\"subsystem\":\"{}\",\"detail\":\"{}\"}}",
            self.ts_ms,
            self.kind.as_str(),
            self.severity.as_str(),
            self.subsystem,
            d,
        )
    }
}

/// Map a watchdog subsystem name to a typed `FailureRecord`.
pub fn classify_incident(subsystem: &'static str, detail: &str) -> FailureRecord {
    let (kind, severity) = match subsystem {
        "recorder_frozen" | "audio_device" => (FailureKind::Audio, FailureSeverity::Error),
        "ipc_silent" | "ipc_client" => (FailureKind::Ipc, FailureSeverity::Warning),
        "wake_engine" => (FailureKind::Wake, FailureSeverity::Error),
        "stt_worker" | "recognizer" | "stt_worker_disconnected" => {
            (FailureKind::Stt, FailureSeverity::Error)
        }
        "zombie_wake" | "zombie_command" => (FailureKind::Lifecycle, FailureSeverity::Warning),
        "cooldown_stuck" | "awaiting_chain_stuck" => (FailureKind::Timing, FailureSeverity::Warning),
        "speaking_gate" | "gate_rapid_resets" => {
            (FailureKind::StateCorruption, FailureSeverity::Warning)
        }
        "recovery_storm" => (FailureKind::Lifecycle, FailureSeverity::Critical),
        "command_exec" => (FailureKind::CommandExec, FailureSeverity::Error),
        "lua_sandbox" => (FailureKind::LuaSandbox, FailureSeverity::Error),
        "replay" => (FailureKind::Replay, FailureSeverity::Info),
        _ => (FailureKind::Lifecycle, FailureSeverity::Warning),
    };
    FailureRecord::new(kind, severity, subsystem, detail)
}
