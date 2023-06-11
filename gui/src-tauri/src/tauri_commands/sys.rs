use peak_alloc::PeakAlloc;

#[global_allocator]
static PEAK_ALLOC: PeakAlloc = PeakAlloc;

extern crate systemstat;
use std::thread;
use std::time::Duration;
use systemstat::{Platform, System};

lazy_static! {
    static ref SYS: System = System::new();
}

#[tauri::command]
pub fn get_current_ram_usage() -> String {
    let result = String::from(format!("{}", PEAK_ALLOC.current_usage_as_mb()));

    result
}

#[tauri::command]
pub fn get_peak_ram_usage() -> String {
    let result = String::from(format!("{}", PEAK_ALLOC.peak_usage_as_gb()));

    result
}

#[tauri::command]
pub fn get_cpu_temp() -> String {
    if let Ok(cpu_temp) = SYS.cpu_temp() {
        String::from(format!("{}", cpu_temp))
    } else {
        String::from("error")
    }
}

// https://github.com/valpackett/systemstat/blob/trunk/examples/info.rs
#[tauri::command(async)]
pub async fn get_cpu_usage() -> String {
    if let Ok(cpu) = SYS.cpu_load_aggregate() {
        thread::sleep(Duration::from_secs(1));
        let cpu = cpu.done().unwrap();
        String::from(format!("{}", cpu.user * 100.0))
    } else {
        String::from("error")
    }
}
