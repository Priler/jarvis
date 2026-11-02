use std::{collections::HashMap, path::PathBuf};

use crate::commands::SlotValue;

// Context passed to Lua scripts
#[derive(Debug, Clone)]
pub struct CommandContext {
    // The phrase that triggered the command
    pub phrase: String,

    // Command ID
    pub command_id: String,

    // Path to command folder
    pub command_path: PathBuf,

    // Current language
    pub language: String,

    // Slots
    pub slots: Option<HashMap<String, SlotValue>>,
}

// Result returned from Lua script execution
#[derive(Debug, Clone)]
pub struct CommandResult {
    // Whether to continue chaining commands
    pub chain: bool,
}

impl Default for CommandResult {
    fn default() -> Self {
        Self { chain: true }
    }
}
