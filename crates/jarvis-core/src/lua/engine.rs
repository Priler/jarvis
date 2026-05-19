use mlua::{Lua, Value, StdLib};
use std::path::PathBuf;
use std::time::Duration;
use std::fs;

use super::sandbox::SandboxLevel;
use super::error::LuaError;
use super::{CommandContext, CommandResult};
use super::api;

pub struct LuaEngine {
    lua: Lua,
    sandbox: SandboxLevel,
}



impl LuaEngine {
    pub fn new(sandbox: SandboxLevel) -> Result<Self, LuaError> {
        // select which standard libraries to load based on sandbox access level
        let std_libs = match sandbox {
            SandboxLevel::Minimal => {
                StdLib::TABLE | StdLib::STRING | StdLib::MATH
            }
            SandboxLevel::Standard => {
                StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8
            }
            SandboxLevel::Full => {
                StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8 | StdLib::OS
            }
        };
        
        let lua = Lua::new_with(std_libs, mlua::LuaOptions::default())
            .map_err(|e| LuaError::InitError(e.to_string()))?;
        
        // remove dangerous globals regardless of sandbox
        {
            let globals = lua.globals();
            
            // always remove these
            let _ = globals.set("loadfile", Value::Nil);
            let _ = globals.set("dofile", Value::Nil);
            let _ = globals.set("load", Value::Nil);
            let _ = globals.set("loadstring", Value::Nil);
            
            // remove io unless full sandbox
            if !matches!(sandbox, SandboxLevel::Full) {
                let _ = globals.set("io", Value::Nil);
            }
            
            // remove os.execute, os.exit, os.setlocale even in full mode
            // for SECURITY REASONS!!!
            if matches!(sandbox, SandboxLevel::Full) {
                if let Ok(os) = globals.get::<mlua::Table>("os") {
                    let _ = os.set("execute", Value::Nil);
                    let _ = os.set("exit", Value::Nil);
                    let _ = os.set("remove", Value::Nil);
                    let _ = os.set("rename", Value::Nil);
                    let _ = os.set("setlocale", Value::Nil);
                }
            }
        }
        
        Ok(Self { lua, sandbox })
    }
    
    // Register all jarvis APIs
    fn register_api(&self, context: &CommandContext) -> Result<(), LuaError> {
        let globals = self.lua.globals();
        
        // main jarvis table
        let jarvis = self.lua.create_table()
            .map_err(|e| LuaError::InitError(e.to_string()))?;
        
        // always register core APIs
        api::core::register(&self.lua, &jarvis)?;
        api::audio::register(&self.lua, &jarvis)?;
        api::context::register(&self.lua, &jarvis, context)?;
        
        // sandbox-controlled APIs
        if self.sandbox.allows_http() {
            api::http::register(&self.lua, &jarvis, self.sandbox)?;
        }
        
        if self.sandbox.allows_state() {
            api::state::register(&self.lua, &jarvis, &context.command_path)?;
        }
        
        if self.sandbox.allows_fs() {
            api::fs::register(&self.lua, &jarvis, &context.command_path, self.sandbox)?;
        }
        
        api::system::register(&self.lua, &jarvis, self.sandbox)?;
        
        globals.set("jarvis", jarvis)
            .map_err(|e| LuaError::InitError(e.to_string()))?;
        
        Ok(())
    }
    
    // Main LUA executor
    pub fn execute(
        &self,
        script_path: &PathBuf,
        context: CommandContext,
        timeout: Duration,
    ) -> Result<CommandResult, LuaError> {
        // register APIs
        self.register_api(&context)?;
        
        // load script
        let script_content = fs::read_to_string(script_path)
            .map_err(|e| LuaError::LoadError(format!("{}: {}", script_path.display(), e)))?;
        
        let script_name = script_path.file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();


        // set up timeout hook
        let start = std::time::Instant::now();
        self.lua.set_hook(mlua::HookTriggers {
            every_nth_instruction: Some(1000),
            ..Default::default()
        }, move |_lua, _debug| {
            if start.elapsed() > timeout {
                Err(mlua::Error::runtime("Script timeout"))
            } else {
                Ok(mlua::VmState::Continue)
            }
        }).map_err(|e| LuaError::InitError(e.to_string()))?;

        // execute script
        let result = self.lua.load(&script_content)
            .set_name(&script_name)
            .eval::<Value>();
        
        // remove hook
        let _ = self.lua.remove_hook();

        // result
        match result {
            Ok(value) => Ok(self.parse_result(value)),
            Err(e) => {
                if e.to_string().contains("timeout") {
                    Err(LuaError::Timeout)
                } else {
                    Err(LuaError::RuntimeError(e.to_string()))
                }
            }
        }
    }
    
    // Parse Lua return value into CommandResult
    fn parse_result(&self, value: Value) -> CommandResult {
        match value {
            // return { chain = false }
            Value::Table(t) => {
                let chain = t.get::<bool>("chain").unwrap_or(true);
                CommandResult { chain }
            }
            // return false (shorthand for no chain)
            Value::Boolean(chain) => CommandResult { chain },
            // return nil or no return = chain
            _ => CommandResult::default(),
        }
    }
}