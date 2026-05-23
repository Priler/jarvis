//! Runtime hardening — tracks module crash counts and auto-disables unstable modules.
//! A module is disabled when it crashes ≥ CRASH_THRESHOLD times within CRASH_WINDOW_MS.

use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static MODULES_DISABLED:   AtomicU64 = AtomicU64::new(0);
pub static TOTAL_CRASHES:      AtomicU64 = AtomicU64::new(0);
pub static RECOVERY_ATTEMPTS:  AtomicU64 = AtomicU64::new(0);

const CRASH_THRESHOLD: usize = 3;
const CRASH_WINDOW_MS: u64   = 60_000;  // 60 seconds
const MAX_MODULES:     usize = 200;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleHealth {
    pub name:         String,
    pub crash_count:  usize,
    pub disabled:     bool,
    pub last_crash:   u64,
    pub disabled_at:  Option<u64>,
}

struct HardeningState {
    modules:  HashMap<String, ModuleHealth>,
}

static STATE: Lazy<Mutex<HardeningState>> = Lazy::new(|| Mutex::new(HardeningState {
    modules: HashMap::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Record a crash for a module. Auto-disables if threshold is exceeded.
pub fn record_crash(module: &str) {
    TOTAL_CRASHES.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();
    let mut s = STATE.lock().unwrap();

    if s.modules.len() >= MAX_MODULES {
        // Evict oldest disabled module
        if let Some(k) = s.modules.values()
            .filter(|m| m.disabled)
            .min_by_key(|m| m.disabled_at.unwrap_or(0))
            .map(|m| m.name.clone()) {
            s.modules.remove(&k);
        }
    }

    let entry = s.modules.entry(module.to_string()).or_insert(ModuleHealth {
        name:        module.to_string(),
        crash_count: 0,
        disabled:    false,
        last_crash:  0,
        disabled_at: None,
    });

    // Reset counter if window has elapsed
    if now.saturating_sub(entry.last_crash) > CRASH_WINDOW_MS {
        entry.crash_count = 0;
    }

    entry.crash_count += 1;
    entry.last_crash   = now;

    if !entry.disabled && entry.crash_count >= CRASH_THRESHOLD {
        entry.disabled    = true;
        entry.disabled_at = Some(now);
        MODULES_DISABLED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Check if a module is currently disabled.
pub fn is_disabled(module: &str) -> bool {
    STATE.lock().unwrap()
        .modules.get(module)
        .map(|m| m.disabled)
        .unwrap_or(false)
}

/// Attempt to re-enable a module after recovery.
pub fn attempt_recovery(module: &str) -> bool {
    RECOVERY_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    let mut s = STATE.lock().unwrap();
    if let Some(entry) = s.modules.get_mut(module) {
        if entry.disabled {
            entry.disabled    = false;
            entry.crash_count = 0;
            entry.disabled_at = None;
            let count = MODULES_DISABLED.load(Ordering::Relaxed);
            if count > 0 { MODULES_DISABLED.fetch_sub(1, Ordering::Relaxed); }
            return true;
        }
    }
    false
}

pub fn get_module_health(module: &str) -> Option<ModuleHealth> {
    STATE.lock().unwrap().modules.get(module).cloned()
}

pub fn all_disabled() -> Vec<ModuleHealth> {
    STATE.lock().unwrap().modules.values().filter(|m| m.disabled).cloned().collect()
}

pub fn check_all() -> Vec<ModuleHealth> {
    STATE.lock().unwrap().modules.values().cloned().collect()
}

pub fn modules_disabled()  -> u64 { MODULES_DISABLED.load(Ordering::Relaxed) }
pub fn total_crashes()     -> u64 { TOTAL_CRASHES.load(Ordering::Relaxed) }
pub fn recovery_attempts() -> u64 { RECOVERY_ATTEMPTS.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_not_disabled_initially() {
        assert!(!is_disabled("test_module_fresh"));
    }

    #[test]
    fn module_disabled_after_threshold() {
        let m = "threshold_test_module_xyz";
        for _ in 0..CRASH_THRESHOLD {
            record_crash(m);
        }
        assert!(is_disabled(m));
        // Cleanup
        attempt_recovery(m);
    }

    #[test]
    fn recovery_re_enables_module() {
        let m = "recovery_test_module_abc";
        for _ in 0..CRASH_THRESHOLD { record_crash(m); }
        assert!(is_disabled(m));
        assert!(attempt_recovery(m));
        assert!(!is_disabled(m));
    }

    #[test]
    fn check_all_no_panic() {
        let _ = check_all();
    }

    #[test]
    fn total_crashes_increases() {
        let before = TOTAL_CRASHES.load(Ordering::Relaxed);
        record_crash("counter_test_module");
        assert!(TOTAL_CRASHES.load(Ordering::Relaxed) > before);
    }
}
