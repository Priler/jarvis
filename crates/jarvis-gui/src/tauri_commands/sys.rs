use sysinfo::{System, Pid, ProcessRefreshKind, RefreshKind, CpuRefreshKind, Components};
use peak_alloc::PeakAlloc;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use log::{info, error};

#[global_allocator]
static PEAK_ALLOC: PeakAlloc = PeakAlloc;

static SYS: Lazy<Mutex<System>> = Lazy::new(|| {
    Mutex::new(System::new_with_specifics(
        RefreshKind::nothing()
            .with_processes(ProcessRefreshKind::nothing().with_memory().with_cpu())
            .with_cpu(CpuRefreshKind::everything())
    ))
});

static COMPONENTS: Lazy<Mutex<Components>> = Lazy::new(|| {
    Mutex::new(Components::new_with_refreshed_list())
});

// Cached PID so repeated polls use targeted refresh instead of scanning all processes.
static CACHED_PID: Lazy<Mutex<Option<Pid>>> = Lazy::new(|| Mutex::new(None));

const JARVIS_APP_NAME: &str = "jarvis-app";

/// Find jarvis-app by scanning all processes (full scan fallback).
fn find_jarvis_app_pid(sys: &System) -> Option<Pid> {
    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains(JARVIS_APP_NAME) {
            return Some(*pid);
        }
    }
    None
}

/// Return the jarvis-app PID, using a targeted refresh when a cached PID is available.
/// Falls back to a full scan only when the cache is empty or the process is gone.
fn get_or_refresh_pid(sys: &mut System) -> Option<Pid> {
    let cached = *CACHED_PID.lock().unwrap();

    if let Some(pid) = cached {
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        if sys.process(pid).map_or(false, |p| {
            p.name().to_string_lossy().to_lowercase().contains(JARVIS_APP_NAME)
        }) {
            return Some(pid);
        }
        // Process is gone — clear cache and fall through to full scan.
        *CACHED_PID.lock().unwrap() = None;
    }

    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let pid = find_jarvis_app_pid(sys);
    *CACHED_PID.lock().unwrap() = pid;
    pid
}

#[derive(serde::Serialize)]
pub struct JarvisAppStats {
    pub running: bool,
    pub ram_mb: u64,
    pub cpu_usage: f32,
}

#[tauri::command]
pub fn get_jarvis_app_stats() -> JarvisAppStats {
    let mut sys = SYS.lock().unwrap();

    if let Some(pid) = get_or_refresh_pid(&mut sys) {
        if let Some(proc) = sys.process(pid) {
            return JarvisAppStats {
                running: true,
                ram_mb: proc.memory() / 1024 / 1024,
                cpu_usage: proc.cpu_usage(),
            };
        }
    }

    JarvisAppStats {
        running: false,
        ram_mb: 0,
        cpu_usage: 0.0,
    }
}

#[tauri::command]
pub fn get_current_ram_usage() -> u64 {
    let mut sys = SYS.lock().unwrap();

    if let Some(pid) = get_or_refresh_pid(&mut sys) {
        if let Some(proc) = sys.process(pid) {
            return proc.memory() / 1024 / 1024;
        }
    }

    0
}

#[tauri::command]
pub fn is_jarvis_app_running() -> bool {
    let mut sys = SYS.lock().unwrap();
    get_or_refresh_pid(&mut sys).is_some()
}

#[tauri::command]
pub fn get_cpu_temp() -> String {
    let mut components = COMPONENTS.lock().unwrap();
    components.refresh(true);

    for component in components.iter() {
        let label = component.label().to_lowercase();
        if label.contains("cpu") || label.contains("core") || label.contains("package") {
            if let Some(temp) = component.temperature() {
                return format!("{:.1}", temp);
            }
        }
    }

    if let Some(component) = components.iter().next() {
        if let Some(temp) = component.temperature() {
            return format!("{:.1}", temp);
        }
    }

    String::from("N/A")
}

#[tauri::command]
pub fn get_cpu_usage() -> f32 {
    let mut sys = SYS.lock().unwrap();

    sys.refresh_cpu_all();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_all();

    sys.global_cpu_usage()
}

#[tauri::command]
pub fn get_peak_ram_usage() -> String {
    format!("{}", PEAK_ALLOC.peak_usage_as_gb())
}

#[tauri::command]
pub fn run_jarvis_app() -> Result<(), String> {
    // Kill any existing instance so the new one can acquire the microphone.
    {
        let mut sys = SYS.lock().unwrap();
        if let Some(pid) = get_or_refresh_pid(&mut sys) {
            if let Some(proc) = sys.process(pid) {
                proc.kill();
            }
        }
        // Invalidate cache — the old process is dead and a new one will get a different PID.
        *CACHED_PID.lock().unwrap() = None;
    }
    std::thread::sleep(std::time::Duration::from_millis(500));

    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("Failed to get exe path: {}", e))?
        .parent()
        .ok_or("Failed to get exe directory")?
        .to_path_buf();

    #[cfg(target_os = "windows")]
    let jarvis_app_name = "jarvis-app.exe";

    #[cfg(not(target_os = "windows"))]
    let jarvis_app_name = "jarvis-app";

    let jarvis_app_path = exe_dir.join(jarvis_app_name);

    info!("Launching jarvis-app subprocess: {}", jarvis_app_path.display());

    if !jarvis_app_path.exists() {
        error!("jarvis-app binary not found at: {}", jarvis_app_path.display());
        return Err(format!("jarvis-app not found at: {}", jarvis_app_path.display()));
    }

    match std::process::Command::new(&jarvis_app_path)
        .current_dir(&exe_dir)
        .spawn()
    {
        Ok(child) => {
            info!("jarvis-app spawned successfully (PID {})", child.id());
            Ok(())
        }
        Err(e) => {
            error!("Failed to spawn jarvis-app: {}", e);
            Err(format!("Failed to start jarvis-app: {}", e))
        }
    }
}
