// System Lua API: exec, open, clipboard, notify

use mlua::{Lua, Table};
use std::process::Command;

use crate::lua::sandbox::SandboxLevel;

/// SEC-9: Allow known-safe URL schemes and whitelisted local file extensions.
/// Blocks shell metacharacters by rejecting anything that isn't a recognised
/// scheme or a local file with a non-executable extension.
fn is_safe_open_target(target: &str) -> bool {
    let t = target.to_lowercase();

    // Allow standard URL schemes.
    if t.starts_with("https://")
        || t.starts_with("http://")
        || t.starts_with("ftp://")
        || t.starts_with("mailto:")
    {
        return true;
    }

    // Allow local files with safe, non-executable extensions.
    // Executables (.exe, .bat, .ps1, .cmd, .vbs, .js, .msi, …) are intentionally absent.
    if let Some(ext) = std::path::Path::new(target).extension().and_then(|e| e.to_str()) {
        return matches!(
            ext.to_lowercase().as_str(),
            "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a"
                | "mp4" | "mkv" | "avi" | "mov" | "webm"
                | "pdf" | "txt" | "log" | "md"
                | "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
                | "lnk"
        );
    }

    false
}

pub fn register(lua: &Lua, jarvis: &Table, sandbox: SandboxLevel) -> mlua::Result<()> {
    let system = lua.create_table()?;

    // jarvis.system.open(url_or_path) - always available
    let open_fn = lua.create_function(|_, target: String| {
        // SEC-9: validate target to prevent shell metachar injection on Windows
        if cfg!(target_os = "windows") && !is_safe_open_target(&target) {
            log::warn!("[Lua] Blocked open() with non-URL target: {}", target);
            return Ok(false);
        }

        let result = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", "start", "", &target])
                .spawn()
        } else if cfg!(target_os = "macos") {
            Command::new("open")
                .arg(&target)
                .spawn()
        } else {
            Command::new("xdg-open")
                .arg(&target)
                .spawn()
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

    // jarvis.system.exec(program, args?) - only in full sandbox.
    // SEC-1: program is passed directly to the OS, never through cmd.exe or sh,
    // so shell metacharacters (& | ; < >) in `program` or `args` are inert.
    // Use the `args` table for arguments rather than embedding them in `program`.
    if sandbox.allows_exec() {
        let exec_fn = lua.create_function(|lua, (program, args): (String, Option<Table>)| {
            if program.trim().is_empty() {
                return Err(mlua::Error::runtime("exec: program name must not be empty"));
            }

            let mut command = Command::new(&program);

            if let Some(args_table) = args {
                for pair in args_table.sequence_values::<String>() {
                    if let Ok(arg) = pair {
                        command.arg(arg);
                    }
                }
            }

            let output = command.output()
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;

            let result = lua.create_table()?;
            result.set("success", output.status.success())?;
            result.set("code", output.status.code().unwrap_or(-1))?;
            result.set("stdout", String::from_utf8_lossy(&output.stdout).to_string())?;
            result.set("stderr", String::from_utf8_lossy(&output.stderr).to_string())?;

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
            use winrt_notification::{Toast, Duration as ToastDuration};

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
            let _ = Command::new("notify-send")
                .args([&title, &message])
                .spawn();
        }

        #[cfg(target_os = "macos")]
        {
            // SEC-8: escape \ before " so that a trailing \ cannot convert the
            // closing delimiter into an escaped quote, enabling code injection.
            let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let script = format!(
                r#"display notification "{}" with title "{}""#,
                escape(&message),
                escape(&title),
            );
            let _ = Command::new("osascript")
                .args(["-e", &script])
                .spawn();
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
                .args(["-NoProfile", "-NonInteractive", "-Command", "Get-Clipboard"])
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
            Err(mlua::Error::runtime("Clipboard not supported on this platform"))
        }
    })?;
    clipboard.set("get", clipboard_get_fn)?;

    // jarvis.system.clipboard.set(text) - only in full sandbox
    if sandbox.allows_clipboard_write() {
        let clipboard_set_fn = lua.create_function(|_, text: String| {
            #[cfg(target_os = "windows")]
            {
                // SEC-8: pass text via stdin so no string interpolation into PowerShell script.
                use std::io::Write;
                let mut child = Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command",
                           "$input | Set-Clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(text.as_bytes())
                        .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                }
                let _ = child.wait();
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
                    stdin.write_all(text.as_bytes())
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
                    stdin.write_all(text.as_bytes())
                        .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                }
            }

            Ok(true)
        })?;
        clipboard.set("set", clipboard_set_fn)?;
    }

    system.set("clipboard", clipboard)?;

    // jarvis.system.env(name) - get environment variable (always available)
    let env_fn = lua.create_function(|_, name: String| {
        Ok(std::env::var(&name).ok())
    })?;
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
