//! Memory pressure guard — monitors memory tier sizes, evicts old entries under
//! pressure, and signals when background indexing should pause.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static EVICTIONS_TRIGGERED:  AtomicU64 = AtomicU64::new(0);
pub static PRESSURE_ALERTS:      AtomicU64 = AtomicU64::new(0);
pub static GUARD_CHECKS:         AtomicU64 = AtomicU64::new(0);
static CURRENT_PRESSURE_PCT:     AtomicU8  = AtomicU8::new(0);

const LOW_WATERMARK:  u8 = 50;
const HIGH_WATERMARK: u8 = 75;
const CRITICAL_MARK:  u8 = 90;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum PressureLevel {
    Normal,
    Elevated,
    High,
    Critical,
}

impl PressureLevel {
    pub fn from_pct(pct: u8) -> Self {
        match pct {
            0..=49  => Self::Normal,
            50..=74 => Self::Elevated,
            75..=89 => Self::High,
            _       => Self::Critical,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Normal   => "Normal",
            Self::Elevated => "Elevated",
            Self::High     => "High",
            Self::Critical => "Critical",
        }
    }
}

struct GuardState {
    conversation_cap: usize,
    project_cap:      usize,
    history:          Vec<(u64, u8)>, // (timestamp, pct) — up to 32
}

impl GuardState {
    fn new() -> Self {
        Self {
            conversation_cap: 500,
            project_cap:      2000,
            history:          Vec::new(),
        }
    }
}

static STATE: Lazy<Mutex<GuardState>> = Lazy::new(|| Mutex::new(GuardState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn update_pressure(pct: u8) {
    GUARD_CHECKS.fetch_add(1, Ordering::Relaxed);
    CURRENT_PRESSURE_PCT.store(pct.min(100), Ordering::Relaxed);

    let mut s = STATE.lock().unwrap();
    if s.history.len() >= 32 { s.history.remove(0); }
    s.history.push((ts_now(), pct));

    if pct >= CRITICAL_MARK {
        PRESSURE_ALERTS.fetch_add(1, Ordering::Relaxed);
        drop(s);
        crate::production_logging::warn("memory_pressure_guard",
            &format!("critical memory pressure: {}%", pct));
        trigger_eviction();
    } else if pct >= HIGH_WATERMARK {
        PRESSURE_ALERTS.fetch_add(1, Ordering::Relaxed);
        drop(s);
        crate::production_logging::warn("memory_pressure_guard",
            &format!("high memory pressure: {}%", pct));
    }
}

fn trigger_eviction() {
    EVICTIONS_TRIGGERED.fetch_add(1, Ordering::Relaxed);
    // Signal memory_runtime to trim conversation tier
    // (actual eviction happens via memory_runtime's tier management)
    crate::notification_center::warn("memory_pressure_guard",
        "Memory pressure high — evicting old conversation entries");
}

pub fn current_pressure_pct()  -> u8           { CURRENT_PRESSURE_PCT.load(Ordering::Relaxed) }
pub fn current_pressure_level() -> PressureLevel { PressureLevel::from_pct(current_pressure_pct()) }
pub fn should_pause_indexing()  -> bool          { current_pressure_pct() >= HIGH_WATERMARK }
pub fn should_evict()           -> bool          { current_pressure_pct() >= CRITICAL_MARK }

pub fn pressure_history() -> Vec<(u64, u8)> {
    STATE.lock().unwrap().history.clone()
}

#[derive(Debug, serde::Serialize)]
pub struct PressureSnapshot {
    pub pressure_pct:       u8,
    pub pressure_level:     String,
    pub evictions_total:    u64,
    pub pressure_alerts:    u64,
    pub guard_checks:       u64,
    pub indexing_paused:    bool,
}

pub fn snapshot() -> PressureSnapshot {
    PressureSnapshot {
        pressure_pct:    current_pressure_pct(),
        pressure_level:  current_pressure_level().label().to_string(),
        evictions_total: EVICTIONS_TRIGGERED.load(Ordering::Relaxed),
        pressure_alerts: PRESSURE_ALERTS.load(Ordering::Relaxed),
        guard_checks:    GUARD_CHECKS.load(Ordering::Relaxed),
        indexing_paused: should_pause_indexing(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_pressure_no_alert() {
        update_pressure(20);
        assert_eq!(current_pressure_level(), PressureLevel::Normal);
    }

    #[test]
    fn high_pressure_pauses_indexing() {
        update_pressure(80);
        assert!(should_pause_indexing());
        update_pressure(0); // restore
    }

    #[test]
    fn critical_triggers_eviction() {
        let before = EVICTIONS_TRIGGERED.load(Ordering::Relaxed);
        update_pressure(95);
        assert!(EVICTIONS_TRIGGERED.load(Ordering::Relaxed) > before);
        update_pressure(0); // restore
    }

    #[test]
    fn pressure_level_from_pct() {
        assert_eq!(PressureLevel::from_pct(30), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_pct(60), PressureLevel::Elevated);
        assert_eq!(PressureLevel::from_pct(80), PressureLevel::High);
        assert_eq!(PressureLevel::from_pct(95), PressureLevel::Critical);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(!s.pressure_level.is_empty());
    }
}
