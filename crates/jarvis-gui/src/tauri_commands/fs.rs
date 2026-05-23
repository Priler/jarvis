#[cfg(target_os = "linux")]
use std::fs::metadata;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use std::process::Command;

use jarvis_core::APP_CONFIG_DIR;

/// Read the per-session IPC auth token written by jarvis-app on startup.
#[tauri::command]
pub fn read_ipc_token() -> Result<String, String> {
    let config_dir = APP_CONFIG_DIR
        .get()
        .ok_or_else(|| "Config dir not initialized".to_string())?;
    let token_path = config_dir.join("ipc_token");
    std::fs::read_to_string(&token_path)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("IPC token not available: {}", e))
}

// Reveal a path in the system file manager.
// SEC-6: path is passed as a discrete argument (never interpolated into a shell string)
// on all platforms. No .unwrap() — failures are silently swallowed so a bad path
// never crashes the GUI process.
#[tauri::command]
pub fn show_in_folder(path: String) {
    #[cfg(target_os = "windows")]
    {
        // /select, and path are separate args; explorer.exe handles special chars fine.
        let _ = Command::new("explorer")
            .args(["/select,", &path])
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        if path.contains(',') {
            // see https://gitlab.freedesktop.org/dbus/dbus/-/issues/76
            let open_path = match metadata(&path).map(|m| m.is_dir()) {
                Ok(true) => path,
                _ => {
                    let mut p = PathBuf::from(path);
                    p.pop();
                    match p.into_os_string().into_string() {
                        Ok(s) => s,
                        Err(_) => return,
                    }
                }
            };
            let _ = Command::new("xdg-open").arg(&open_path).spawn();
        } else {
            // SEC-6: escape " in the path so dbus-send's type-annotation parser
            // cannot be confused by a quote embedded in the file name.
            let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
            let dbus_arg = format!("array:string:\"file://{}\"", escaped);
            let _ = Command::new("dbus-send")
                .args([
                    "--session",
                    "--dest=org.freedesktop.FileManager1",
                    "--type=method_call",
                    "/org/freedesktop/FileManager1",
                    "org.freedesktop.FileManager1.ShowItems",
                    &dbus_arg,
                    "string:\"\"",
                ])
                .spawn();
        }
    }

    #[cfg(target_os = "macos")]
    {
        // -R and path are separate args; no shell involvement.
        let _ = Command::new("open").args(["-R", &path]).spawn();
    }
}