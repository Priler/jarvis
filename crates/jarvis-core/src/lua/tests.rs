#[cfg(test)]
mod tests {
    use crate::lua::{CommandContext, LuaError, SandboxLevel, execute};

    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::tempdir;
    use std::fs;
    
    fn create_test_context(cmd_path: PathBuf) -> CommandContext {
        CommandContext {
            phrase: "test phrase".to_string(),
            command_id: "test_cmd".to_string(),
            command_path: cmd_path,
            language: "en".to_string(),
            slots: None,
        }
    }
    
    #[test]
    fn test_minimal_sandbox() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        
        fs::write(&script_path, r#"
            jarvis.log("info", "test log")
            return { chain = false }
        "#).unwrap();
        
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(
            &script_path,
            context,
            SandboxLevel::Minimal,
            Duration::from_secs(5),
        );
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap().chain, false);
    }
    
    #[test]
    fn test_state_persistence() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        
        // first run - set state
        fs::write(&script_path, r#"
            jarvis.state.set("key", "value")
            return true
        "#).unwrap();
        
        let context = create_test_context(dir.path().to_path_buf());
        execute(&script_path, context, SandboxLevel::Standard, Duration::from_secs(5)).unwrap();
        
        // second run - read state
        fs::write(&script_path, r#"
            local val = jarvis.state.get("key")
            if val == "value" then
                return true
            else
                error("State not persisted")
            end
        "#).unwrap();
        
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Standard, Duration::from_secs(5));
        
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_timeout() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        
        fs::write(&script_path, r#"
            while true do end
        "#).unwrap();
        
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(
            &script_path,
            context,
            SandboxLevel::Minimal,
            Duration::from_millis(100),
        );
        
        assert!(matches!(result, Err(LuaError::Timeout)));
    }
    
    #[test]
    fn test_sandbox_fs_escape() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");

        fs::write(&script_path, r#"
            local ok, err = pcall(function()
                jarvis.fs.read("../../../etc/passwd")
            end)
            if ok then
                error("Should have been blocked")
            end
            return true
        "#).unwrap();

        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Standard, Duration::from_secs(5));

        assert!(result.is_ok());
    }

    // --- P2-2: Sandbox escape tests ---

    #[test]
    fn dangerous_globals_removed_in_all_levels() {
        for sandbox in [SandboxLevel::Minimal, SandboxLevel::Standard, SandboxLevel::Full] {
            let dir = tempdir().unwrap();
            let script_path = dir.path().join("test.lua");
            fs::write(&script_path, r#"
                assert(loadfile == nil, "loadfile must be nil")
                assert(dofile == nil, "dofile must be nil")
                assert(load == nil, "load must be nil")
                return true
            "#).unwrap();
            let context = create_test_context(dir.path().to_path_buf());
            let result = execute(&script_path, context, sandbox, Duration::from_secs(5));
            assert!(result.is_ok(), "sandbox {:?}: {:?}", sandbox, result);
        }
    }

    #[test]
    fn io_blocked_in_non_full_sandboxes() {
        for sandbox in [SandboxLevel::Minimal, SandboxLevel::Standard] {
            let dir = tempdir().unwrap();
            let script_path = dir.path().join("test.lua");
            fs::write(&script_path, r#"
                local ok = pcall(function() io.open("test.txt", "r") end)
                if ok then error("io should not be accessible") end
                return true
            "#).unwrap();
            let context = create_test_context(dir.path().to_path_buf());
            let result = execute(&script_path, context, sandbox, Duration::from_secs(5));
            assert!(result.is_ok(), "sandbox {:?}: {:?}", sandbox, result);
        }
    }

    #[test]
    fn os_execute_blocked_even_in_full_sandbox() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        fs::write(&script_path, r#"
            local ok = pcall(function() os.execute("echo pwned") end)
            if ok then error("os.execute should not be accessible") end
            return true
        "#).unwrap();
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Full, Duration::from_secs(5));
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn fs_api_unavailable_in_minimal_sandbox() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        fs::write(&script_path, r#"
            local ok = pcall(function() jarvis.fs.read("test.txt") end)
            if ok then error("jarvis.fs should not exist in minimal sandbox") end
            return true
        "#).unwrap();
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Minimal, Duration::from_secs(5));
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn http_api_unavailable_in_minimal_sandbox() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        fs::write(&script_path, r#"
            local ok = pcall(function() jarvis.http.get("http://localhost/") end)
            if ok then error("jarvis.http should not exist in minimal sandbox") end
            return true
        "#).unwrap();
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Minimal, Duration::from_secs(5));
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn system_exec_unavailable_in_standard_sandbox() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        fs::write(&script_path, r#"
            local ok = pcall(function() jarvis.system.exec("echo", {"hello"}) end)
            if ok then error("jarvis.system.exec should not exist in standard sandbox") end
            return true
        "#).unwrap();
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Standard, Duration::from_secs(5));
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn http_external_url_blocked_in_standard_sandbox() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");
        fs::write(&script_path, r#"
            local ok = pcall(function() jarvis.http.get("https://example.com/data") end)
            if ok then error("External URLs should be blocked in standard sandbox") end
            return true
        "#).unwrap();
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Standard, Duration::from_secs(5));
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn fs_absolute_path_blocked_in_standard_sandbox() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");

        #[cfg(target_os = "windows")]
        let script = r#"
            local ok = pcall(function() jarvis.fs.read("C:\\Windows\\System32\\drivers\\etc\\hosts") end)
            if ok then error("Absolute paths should be blocked in standard sandbox") end
            return true
        "#;

        #[cfg(not(target_os = "windows"))]
        let script = r#"
            local ok = pcall(function() jarvis.fs.read("/etc/passwd") end)
            if ok then error("Absolute paths should be blocked in standard sandbox") end
            return true
        "#;

        fs::write(&script_path, script).unwrap();
        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(&script_path, context, SandboxLevel::Standard, Duration::from_secs(5));
        assert!(result.is_ok(), "{:?}", result);
    }
}