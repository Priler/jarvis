mod api;
mod engine;
mod error;
mod sandbox;

mod structs;
pub use structs::*;

pub use engine::LuaEngine;
pub use error::LuaError;
pub use sandbox::SandboxLevel;

use std::path::PathBuf;
use std::time::Duration;

#[cfg(test)]
mod tests;

// Execute a Lua command script
pub fn execute(
    script_path: &PathBuf,
    context: CommandContext,
    sandbox: SandboxLevel,
    timeout: Duration,
) -> Result<CommandResult, LuaError> {
    let engine = LuaEngine::new(sandbox)?;
    engine.execute(script_path, context, timeout)
}
