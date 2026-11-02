// System Lua API: exec, open, clipboard, notify

use mlua::{Lua, Table};
use std::process::Command;

use crate::lua::sandbox::SandboxLevel;

pub fn register(lua: &Lua, jarvis: &Table, sandbox: SandboxLevel) -> mlua::Result<()> {
    let system = lua.create_table()?;

    // jarvis.system.open(url_or_path) - always available
    let open_fn = lua.create_function(|_, target: String| {
        let result = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", "start", "", &target])
                .spawn()
        } else if cfg!(target_os = "macos") {
            Command::new("open").arg(&target).spawn()
        } else {
            Command::new("xdg-open").arg(&target).spawn()
        };

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                log::warn!("[Lua] Failed to open {}: {}", target, e);
                Ok(false)
            }
        }
    })?;
    system.set("open", open_fn)?;

    // jarvis.system.exec(cmd, args?) - only in full sandbox
    if sandbox.allows_exec() {
        let exec_fn = lua.create_function(|lua, (cmd, args): (String, Option<Table>)| {
            let mut command = if cfg!(target_os = "windows") {
                let mut c = Command::new("cmd");
                c.args(["/C", &cmd]);
                c
            } else {
                let mut c = Command::new("sh");
                c.args(["-c", &cmd]);
                c
            };

            // add extra args if provided
            if let Some(args_table) = args {
                for pair in args_table.sequence_values::<String>() {
                    if let Ok(arg) = pair {
                        command.arg(arg);
                    }
                }
            }

            let output = command
                .output()
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;

            let result = lua.create_table()?;
            result.set("success", output.status.success())?;
            result.set("code", output.status.code().unwrap_or(-1))?;
            result.set(
                "stdout",
                String::from_utf8_lossy(&output.stdout).to_string(),
            )?;
            result.set(
                "stderr",
                String::from_utf8_lossy(&output.stderr).to_string(),
            )?;

            Ok(result)
        })?;
        system.set("exec", exec_fn)?;
    }

    // jarvis.system.notify(title, message) - always available
    let notify_fn = lua.create_function(|_, (title, message): (String, String)| {
        log::info!("[Lua] NOTIFY: {} - {}", title, message);

        // platform-specific notification
        #[cfg(target_os = "windows")]
        {
            use winrt_notification::{Duration as ToastDuration, Toast};

            if let Err(e) = Toast::new(Toast::POWERSHELL_APP_ID)
                .title(&title)
                .text1(&message)
                .duration(ToastDuration::Short)
                .show()
            {
                log::warn!("[Lua] Failed to show toast notification: {}", e);
                // fallback to msg.exe
                let _ = Command::new("msg")
                    .args(["*", "/time:10", &format!("{}: {}", title, message)])
                    .spawn();
            }
        }

        #[cfg(target_os = "linux")]
        {
            let _ = Command::new("notify-send").args([&title, &message]).spawn();
        }

        #[cfg(target_os = "macos")]
        {
            let script = format!(
                r#"display notification "{}" with title "{}""#,
                message.replace("\"", "\\\""),
                title.replace("\"", "\\\"")
            );
            let _ = Command::new("osascript").args(["-e", &script]).spawn();
        }

        Ok(true)
    })?;
    system.set("notify", notify_fn)?;

    // jarvis.system.clipboard - subtable
    let clipboard = lua.create_table()?;

    // jarvis.system.clipboard.get() - always available
    let clipboard_get_fn = lua.create_function(|_, ()| {
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("powershell")
                .args(["-Command", "Get-Clipboard"])
                .output()
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;

            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }

        #[cfg(target_os = "linux")]
        {
            let output = Command::new("xclip")
                .args(["-selection", "clipboard", "-o"])
                .output()
                .or_else(|_| {
                    Command::new("xsel")
                        .args(["--clipboard", "--output"])
                        .output()
                })
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;

            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }

        #[cfg(target_os = "macos")]
        {
            let output = Command::new("pbpaste")
                .output()
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;

            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err(mlua::Error::runtime(
                "Clipboard not supported on this platform",
            ))
        }
    })?;
    clipboard.set("get", clipboard_get_fn)?;

    // jarvis.system.clipboard.set(text) - only in full sandbox
    if sandbox.allows_clipboard_write() {
        let clipboard_set_fn = lua.create_function(|_, text: String| {
            #[cfg(target_os = "windows")]
            {
                let script = format!("Set-Clipboard -Value '{}'", text.replace("'", "''"));
                Command::new("powershell")
                    .args(["-Command", &script])
                    .output()
                    .map_err(|e| mlua::Error::runtime(e.to_string()))?;
            }

            #[cfg(target_os = "linux")]
            {
                use std::io::Write;
                let mut child = Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .or_else(|_| {
                        Command::new("xsel")
                            .args(["--clipboard", "--input"])
                            .stdin(std::process::Stdio::piped())
                            .spawn()
                    })
                    .map_err(|e| mlua::Error::runtime(e.to_string()))?;

                if let Some(stdin) = child.stdin.as_mut() {
                    stdin
                        .write_all(text.as_bytes())
                        .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                }
            }

            #[cfg(target_os = "macos")]
            {
                use std::io::Write;
                let mut child = Command::new("pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| mlua::Error::runtime(e.to_string()))?;

                if let Some(stdin) = child.stdin.as_mut() {
                    stdin
                        .write_all(text.as_bytes())
                        .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                }
            }

            Ok(true)
        })?;
        clipboard.set("set", clipboard_set_fn)?;
    }

    system.set("clipboard", clipboard)?;

    // jarvis.system.env(name) - get environment variable (always available)
    let env_fn = lua.create_function(|_, name: String| Ok(std::env::var(&name).ok()))?;
    system.set("env", env_fn)?;

    // jarvis.system.platform - read-only string
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };
    system.set("platform", platform)?;

    jarvis.set("system", system)?;

    Ok(())
}
