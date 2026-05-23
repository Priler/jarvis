//! Long-run stability engine — detects runtime degradation, memory growth trends,
//! scheduler starvation, and runaway simulation over extended sessions.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static DEGRADATIONS_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static THROTTLES_APPLIED:     AtomicU64 = AtomicU64::new(0);
pub static STABILITY_CHECKS:      AtomicU64 = AtomicU64::new(0);

const SAMPLE_WINDOW: usize = 16;
const LEAK_THRESHOLD_RATIO: f32 = 0.95; // 95% monotone increase = leak suspected
const STARVATION_THRESHOLD: f32 = 0.40; // 40% missed ticks = starvation
const RUNAWAY_SIM_THROTTLE: f32 = 0.90; // world_sim at ≥90% for 8+ samples = runaway

#[derive(Debug, Clone, serde::Serialize)]
pub enum DegradationKind {
    MemoryLeak,
    SchedulerStarvation,
    RunawaySimulation,
    ModelLatencySpike,
    VoicePipelineDrop,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DegradationEvent {
    pub kind:      DegradationKind,
    pub message:   String,
    pub timestamp: u64,
    pub auto_throttled: bool,
}

struct StabilityState {
    memory_samples:     Vec<usize>,
    tick_miss_samples:  Vec<f32>,
    world_sim_samples:  Vec<f32>,
    voice_conf_samples: Vec<f32>,
    events:             Vec<DegradationEvent>,
    start_ts:           u64,
}

impl StabilityState {
    fn new() -> Self {
        Self {
            memory_samples:     Vec::new(),
            tick_miss_samples:  Vec::new(),
            world_sim_samples:  Vec::new(),
            voice_conf_samples: Vec::new(),
            events:             Vec::new(),
            start_ts:           ts_now(),
        }
    }

    fn push_sample<T: Clone>(buf: &mut Vec<T>, val: T) {
        if buf.len() >= SAMPLE_WINDOW { buf.remove(0); }
        buf.push(val);
    }

    fn is_monotone_increasing(samples: &[usize]) -> bool {
        if samples.len() < 4 { return false; }
        let increasing = samples.windows(2).filter(|w| w[1] >= w[0]).count();
        increasing as f32 / (samples.len() - 1) as f32 >= LEAK_THRESHOLD_RATIO
    }
}

static STATE: Lazy<Mutex<StabilityState>> = Lazy::new(|| Mutex::new(StabilityState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn record_event(kind: DegradationKind, message: String, throttled: bool) {
    let mut s = STATE.lock().unwrap();
    if s.events.len() >= 50 { s.events.remove(0); }
    s.events.push(DegradationEvent { kind, message, timestamp: ts_now(), auto_throttled: throttled });
    DEGRADATIONS_DETECTED.fetch_add(1, Ordering::Relaxed);
    if throttled { THROTTLES_APPLIED.fetch_add(1, Ordering::Relaxed); }
}

// ── Feed samples ──────────────────────────────────────────────────────────────

pub fn sample_memory(entry_count: usize) {
    let mut s = STATE.lock().unwrap();
    StabilityState::push_sample(&mut s.memory_samples, entry_count);
}

pub fn sample_tick_miss_ratio(ratio: f32) {
    let mut s = STATE.lock().unwrap();
    StabilityState::push_sample(&mut s.tick_miss_samples, ratio);
}

pub fn sample_world_sim_throttle(throttle: f32) {
    let mut s = STATE.lock().unwrap();
    StabilityState::push_sample(&mut s.world_sim_samples, throttle);
}

pub fn sample_voice_confidence(conf: f32) {
    let mut s = STATE.lock().unwrap();
    StabilityState::push_sample(&mut s.voice_conf_samples, conf);
}

// ── Analysis ──────────────────────────────────────────────────────────────────

pub fn run_check() {
    STABILITY_CHECKS.fetch_add(1, Ordering::Relaxed);

    // Memory leak detection
    {
        let s = STATE.lock().unwrap();
        let mem = s.memory_samples.clone();
        drop(s);
        if mem.len() >= 8 && StabilityState::is_monotone_increasing(&mem) {
            record_event(
                DegradationKind::MemoryLeak,
                format!("memory entries growing monotonically: {} entries", mem.last().unwrap_or(&0)),
                false,
            );
            crate::production_logging::warn("long_run_stability", "memory growth trend detected");
        }
    }

    // Scheduler starvation
    {
        let s = STATE.lock().unwrap();
        let ticks = s.tick_miss_samples.clone();
        drop(s);
        if ticks.len() >= 4 {
            let avg = ticks.iter().sum::<f32>() / ticks.len() as f32;
            if avg >= STARVATION_THRESHOLD {
                record_event(
                    DegradationKind::SchedulerStarvation,
                    format!("avg tick miss ratio {:.0}%", avg * 100.0),
                    true,
                );
                crate::production_logging::warn("long_run_stability",
                    &format!("scheduler starvation: {:.0}% miss", avg * 100.0));
                crate::performance_profiles::set_mode(
                    crate::performance_profiles::PerformanceMode::Eco);
            }
        }
    }

    // Runaway simulation detection
    {
        let s = STATE.lock().unwrap();
        let sim = s.world_sim_samples.clone();
        drop(s);
        if sim.len() >= 8 && sim.iter().all(|&v| v >= RUNAWAY_SIM_THROTTLE) {
            record_event(
                DegradationKind::RunawaySimulation,
                "world simulation throttle pinned at maximum".to_string(),
                true,
            );
            crate::performance_profiles::set_mode(
                crate::performance_profiles::PerformanceMode::Balanced);
        }
    }

    // Voice pipeline drop
    {
        let s = STATE.lock().unwrap();
        let voice = s.voice_conf_samples.clone();
        drop(s);
        if voice.len() >= 4 {
            let avg = voice.iter().sum::<f32>() / voice.len() as f32;
            if avg < 0.40 {
                record_event(
                    DegradationKind::VoicePipelineDrop,
                    format!("avg voice confidence {:.0}% — possible mic issue", avg * 100.0),
                    false,
                );
            }
        }
    }
}

pub fn recent_events(n: usize) -> Vec<DegradationEvent> {
    let s = STATE.lock().unwrap();
    s.events.iter().rev().take(n).cloned().collect()
}

pub fn session_uptime_ms() -> u64 {
    ts_now().saturating_sub(STATE.lock().unwrap().start_ts)
}

#[derive(Debug, serde::Serialize)]
pub struct StabilitySnapshot {
    pub degradations_detected: u64,
    pub throttles_applied:     u64,
    pub stability_checks:      u64,
    pub session_uptime_ms:     u64,
    pub recent_events:         Vec<DegradationEvent>,
}

pub fn snapshot() -> StabilitySnapshot {
    StabilitySnapshot {
        degradations_detected: DEGRADATIONS_DETECTED.load(Ordering::Relaxed),
        throttles_applied:     THROTTLES_APPLIED.load(Ordering::Relaxed),
        stability_checks:      STABILITY_CHECKS.load(Ordering::Relaxed),
        session_uptime_ms:     session_uptime_ms(),
        recent_events:         recent_events(5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_no_panic() {
        sample_memory(100);
        sample_tick_miss_ratio(0.05);
        sample_world_sim_throttle(0.6);
        sample_voice_confidence(0.88);
        assert!(STABILITY_CHECKS.load(Ordering::Relaxed) < u64::MAX);
    }

    #[test]
    fn run_check_no_panic() {
        run_check();
        assert!(STABILITY_CHECKS.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn leak_detection_triggers() {
        let increasing: Vec<usize> = (100..120).collect();
        assert!(StabilityState::is_monotone_increasing(&increasing));
    }

    #[test]
    fn stable_memory_no_leak() {
        let flat: Vec<usize> = vec![100, 99, 101, 98, 100, 102, 100, 99];
        assert!(!StabilityState::is_monotone_increasing(&flat));
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        let _ = s.session_uptime_ms;
    }
}
