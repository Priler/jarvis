//! Performance profiles — throttle or boost runtime components based on user intent
//! and system constraints. No AGI logic; pure configuration dispatch.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PerformanceMode {
    Eco,
    Balanced,
    Performance,
    Reasoning,
    VoicePriority,
    LowVRAM,
}

pub static MODE_CHANGES: AtomicU64 = AtomicU64::new(0);

struct ProfileState {
    mode:                    PerformanceMode,
    world_sim_throttle:      f32,
    reasoning_depth:         u8,
    gpu_alloc_pct:           u8,
    memory_pressure_cap_pct: u8,
    voice_latency_target_ms: u64,
}

impl ProfileState {
    fn from_mode(mode: PerformanceMode) -> Self {
        match mode {
            PerformanceMode::Eco => Self {
                mode, world_sim_throttle: 0.20, reasoning_depth: 3,
                gpu_alloc_pct: 30, memory_pressure_cap_pct: 50, voice_latency_target_ms: 300,
            },
            PerformanceMode::Balanced => Self {
                mode, world_sim_throttle: 0.60, reasoning_depth: 6,
                gpu_alloc_pct: 60, memory_pressure_cap_pct: 70, voice_latency_target_ms: 200,
            },
            PerformanceMode::Performance => Self {
                mode, world_sim_throttle: 1.00, reasoning_depth: 10,
                gpu_alloc_pct: 90, memory_pressure_cap_pct: 90, voice_latency_target_ms: 150,
            },
            PerformanceMode::Reasoning => Self {
                mode, world_sim_throttle: 0.30, reasoning_depth: 10,
                gpu_alloc_pct: 80, memory_pressure_cap_pct: 80, voice_latency_target_ms: 250,
            },
            PerformanceMode::VoicePriority => Self {
                mode, world_sim_throttle: 0.20, reasoning_depth: 4,
                gpu_alloc_pct: 40, memory_pressure_cap_pct: 60, voice_latency_target_ms: 100,
            },
            PerformanceMode::LowVRAM => Self {
                mode, world_sim_throttle: 0.40, reasoning_depth: 5,
                gpu_alloc_pct: 20, memory_pressure_cap_pct: 60, voice_latency_target_ms: 250,
            },
        }
    }
}

static STATE: Lazy<Mutex<ProfileState>> =
    Lazy::new(|| Mutex::new(ProfileState::from_mode(PerformanceMode::Balanced)));

pub fn set_mode(mode: PerformanceMode) {
    let mut s = STATE.lock().unwrap();
    *s = ProfileState::from_mode(mode);
    MODE_CHANGES.fetch_add(1, Ordering::Relaxed);
    crate::production_logging::info(
        "performance_profiles",
        &format!("mode={}", mode_name_static(mode)),
    );
}

pub fn current_mode() -> PerformanceMode { STATE.lock().unwrap().mode }

pub fn world_sim_throttle()      -> f32  { STATE.lock().unwrap().world_sim_throttle }
pub fn reasoning_depth()         -> u8   { STATE.lock().unwrap().reasoning_depth }
pub fn gpu_alloc_pct()           -> u8   { STATE.lock().unwrap().gpu_alloc_pct }
pub fn memory_pressure_cap_pct() -> u8   { STATE.lock().unwrap().memory_pressure_cap_pct }
pub fn voice_latency_target_ms() -> u64  { STATE.lock().unwrap().voice_latency_target_ms }

fn mode_name_static(m: PerformanceMode) -> &'static str {
    match m {
        PerformanceMode::Eco           => "Eco",
        PerformanceMode::Balanced      => "Balanced",
        PerformanceMode::Performance   => "Performance",
        PerformanceMode::Reasoning     => "Reasoning",
        PerformanceMode::VoicePriority => "VoicePriority",
        PerformanceMode::LowVRAM       => "LowVRAM",
    }
}

pub fn current_mode_name() -> &'static str { mode_name_static(current_mode()) }

pub fn all_modes() -> Vec<&'static str> {
    vec!["Eco", "Balanced", "Performance", "Reasoning", "VoicePriority", "LowVRAM"]
}

#[derive(Debug, serde::Serialize)]
pub struct ProfileSnapshot {
    pub mode:                    String,
    pub world_sim_throttle:      f32,
    pub reasoning_depth:         u8,
    pub gpu_alloc_pct:           u8,
    pub memory_pressure_cap_pct: u8,
    pub voice_latency_target_ms: u64,
    pub mode_changes:            u64,
}

pub fn snapshot() -> ProfileSnapshot {
    let s = STATE.lock().unwrap();
    ProfileSnapshot {
        mode:                    format!("{:?}", s.mode),
        world_sim_throttle:      s.world_sim_throttle,
        reasoning_depth:         s.reasoning_depth,
        gpu_alloc_pct:           s.gpu_alloc_pct,
        memory_pressure_cap_pct: s.memory_pressure_cap_pct,
        voice_latency_target_ms: s.voice_latency_target_ms,
        mode_changes:            MODE_CHANGES.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;

    static TEST_LOCK: Lazy<std::sync::Mutex<()>> = Lazy::new(|| std::sync::Mutex::new(()));

    #[test]
    fn balanced_is_default() {
        let _g = TEST_LOCK.lock().unwrap();
        set_mode(PerformanceMode::Balanced);
        assert_eq!(current_mode(), PerformanceMode::Balanced);
    }

    #[test]
    fn eco_throttles_world_sim() {
        let _g = TEST_LOCK.lock().unwrap();
        set_mode(PerformanceMode::Eco);
        assert!(world_sim_throttle() < 0.5);
        set_mode(PerformanceMode::Balanced);
    }

    #[test]
    fn voice_priority_lowest_latency() {
        let vp   = ProfileState::from_mode(PerformanceMode::VoicePriority);
        let perf = ProfileState::from_mode(PerformanceMode::Performance);
        assert!(vp.voice_latency_target_ms <= perf.voice_latency_target_ms);
    }

    #[test]
    fn low_vram_has_lowest_gpu_alloc() {
        let lv   = ProfileState::from_mode(PerformanceMode::LowVRAM);
        let perf = ProfileState::from_mode(PerformanceMode::Performance);
        assert!(lv.gpu_alloc_pct < perf.gpu_alloc_pct);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(!s.mode.is_empty());
        assert!(s.reasoning_depth > 0);
    }

    #[test]
    fn all_modes_listed() {
        assert_eq!(all_modes().len(), 6);
    }
}
