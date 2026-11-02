#[cfg(test)]
mod tests {
    use crate::lua::{execute, CommandContext, LuaError, SandboxLevel};

    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::tempdir;

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

        fs::write(
            &script_path,
            r#"
            jarvis.log("info", "test log")
            return { chain = false }
        "#,
        )
        .unwrap();

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
        fs::write(
            &script_path,
            r#"
            jarvis.state.set("key", "value")
            return true
        "#,
        )
        .unwrap();

        let context = create_test_context(dir.path().to_path_buf());
        execute(
            &script_path,
            context,
            SandboxLevel::Standard,
            Duration::from_secs(5),
        )
        .unwrap();

        // second run - read state
        fs::write(
            &script_path,
            r#"
            local val = jarvis.state.get("key")
            if val == "value" then
                return true
            else
                error("State not persisted")
            end
        "#,
        )
        .unwrap();

        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(
            &script_path,
            context,
            SandboxLevel::Standard,
            Duration::from_secs(5),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_timeout() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.lua");

        fs::write(
            &script_path,
            r#"
            while true do end
        "#,
        )
        .unwrap();

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

        fs::write(
            &script_path,
            r#"
            local ok, err = pcall(function()
                jarvis.fs.read("../../../etc/passwd")
            end)
            if ok then
                error("Should have been blocked")
            end
            return true
        "#,
        )
        .unwrap();

        let context = create_test_context(dir.path().to_path_buf());
        let result = execute(
            &script_path,
            context,
            SandboxLevel::Standard,
            Duration::from_secs(5),
        );

        assert!(result.is_ok());
    }
}
