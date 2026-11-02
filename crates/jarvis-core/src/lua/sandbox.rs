use serde::{Deserialize, Serialize};

// Sandbox level controlling what APIs are available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxLevel {
    // Minimal: only core APIs (log, speak, audio, context)
    Minimal,

    // Standard: + http, state, fs (command folder only)
    Standard,

    // Full: + system.exec, expanded fs access
    Full,
}

impl SandboxLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "minimal" => SandboxLevel::Minimal,
            "full" => SandboxLevel::Full,
            _ => SandboxLevel::Standard,
        }
    }

    // Can use HTTP API
    pub fn allows_http(&self) -> bool {
        matches!(self, SandboxLevel::Standard | SandboxLevel::Full)
    }

    // Can use persistent state API
    pub fn allows_state(&self) -> bool {
        matches!(self, SandboxLevel::Standard | SandboxLevel::Full)
    }

    // Can use file system API
    pub fn allows_fs(&self) -> bool {
        matches!(self, SandboxLevel::Standard | SandboxLevel::Full)
    }

    // Can write files
    pub fn allows_fs_write(&self) -> bool {
        matches!(self, SandboxLevel::Standard | SandboxLevel::Full)
    }

    // Can execute system commands
    pub fn allows_exec(&self) -> bool {
        matches!(self, SandboxLevel::Full)
    }

    // Can access clipboard write
    pub fn allows_clipboard_write(&self) -> bool {
        matches!(self, SandboxLevel::Full)
    }

    // Can access paths outside command folder
    pub fn allows_expanded_paths(&self) -> bool {
        matches!(self, SandboxLevel::Full)
    }
}

impl Default for SandboxLevel {
    fn default() -> Self {
        SandboxLevel::Standard
    }
}
