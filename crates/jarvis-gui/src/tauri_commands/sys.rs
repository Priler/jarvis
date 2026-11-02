use once_cell::sync::Lazy;
use peak_alloc::PeakAlloc;
use std::env;
use std::process::Command;
use std::sync::Mutex;
use sysinfo::{Components, CpuRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};

#[global_allocator]
static PEAK_ALLOC: PeakAlloc = PeakAlloc;

static SYS: Lazy<Mutex<System>> = Lazy::new(|| {
    Mutex::new(System::new_with_specifics(
        RefreshKind::nothing()
            .with_processes(ProcessRefreshKind::nothing().with_memory().with_cpu())
            .with_cpu(CpuRefreshKind::everything()),
    ))
});

static COMPONENTS: Lazy<Mutex<Components>> =
    Lazy::new(|| Mutex::new(Components::new_with_refreshed_list()));

const JARVIS_APP_NAME: &str = "jarvis-app";

/// Find jarvis-app process and return its PID
fn find_jarvis_app_pid(sys: &System) -> Option<Pid> {
    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains(JARVIS_APP_NAME) {
            return Some(*pid);
        }
    }
    None
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

    // refresh all processes to find jarvis-app
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    if let Some(pid) = find_jarvis_app_pid(&sys) {
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
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    if let Some(pid) = find_jarvis_app_pid(&sys) {
        if let Some(proc) = sys.process(pid) {
            return proc.memory() / 1024 / 1024;
        }
    }

    0
}

#[tauri::command]
pub fn is_jarvis_app_running() -> bool {
    let mut sys = SYS.lock().unwrap();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    find_jarvis_app_pid(&sys).is_some()
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

    if !jarvis_app_path.exists() {
        return Err(format!(
            "jarvis-app not found at: {}",
            jarvis_app_path.display()
        ));
    }

    std::process::Command::new(&jarvis_app_path)
        .spawn()
        .map_err(|e| format!("Failed to start jarvis-app: {}", e))?;

    Ok(())
}
