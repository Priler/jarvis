use std::fmt;

#[derive(Debug)]
pub enum LuaError {
    // Failed to create Lua VM
    InitError(String),

    // Failed to load script file
    LoadError(String),

    // Script execution error
    RuntimeError(String),

    // Script exceeded timeout
    Timeout,

    // Sandbox violation
    SandboxViolation(String),

    // IO error
    IoError(std::io::Error),
}

impl fmt::Display for LuaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuaError::InitError(msg) => write!(f, "Lua init error: {}", msg),
            LuaError::LoadError(msg) => write!(f, "Script load error: {}", msg),
            LuaError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
            LuaError::Timeout => write!(f, "Script timeout"),
            LuaError::SandboxViolation(msg) => write!(f, "Sandbox violation: {}", msg),
            LuaError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for LuaError {}

impl From<mlua::Error> for LuaError {
    fn from(e: mlua::Error) -> Self {
        LuaError::RuntimeError(e.to_string())
    }
}

impl From<std::io::Error> for LuaError {
    fn from(e: std::io::Error) -> Self {
        LuaError::IoError(e)
    }
}
