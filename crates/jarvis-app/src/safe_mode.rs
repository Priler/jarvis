//! Safe mode — minimal runtime with only core services active.
//! When entered, disables heavy cognitive layers and world simulation.

use std::sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use once_cell::sync::Lazy;

pub static SAFE_MODE_ENTRIES:  AtomicU64 = AtomicU64::new(0);
pub static SAFE_MODE_EXITS:    AtomicU64 = AtomicU64::new(0);
static SAFE_MODE_ACTIVE:       AtomicBool = AtomicBool::new(false);

// Services allowed in safe mode (minimal set)
const SAFE_MODE_SERVICES: &[&str] = &[
    "stt",           // Speech-to-text (core)
    "belief_engine", // Basic belief tracking
    "memory_runtime", // Memory access
    "permission_runtime", // Security gate
    "production_logging", // Logging always on
    "model_manager",  // Model detection
];

// Services disabled in safe mode
const DISABLED_IN_SAFE_MODE: &[&str] = &[
    "world_simulation_runtime",
    "ai_kernel",
    "evolution_runtime",
    "abstraction_runtime",
    "symbolic_runtime",
    "probabilistic_runtime",
    "hierarchical_runtime",
    "live_meta_loop",
    "rag_pipeline",
    "embedding_runtime",
    "diagnostics_center",
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct SafeModeState {
    pub active:           bool,
    pub entered_at:       Option<u64>,
    pub reason:           String,
    pub allowed_services: Vec<String>,
    pub disabled_services: Vec<String>,
}

static STATE: Lazy<Mutex<SafeModeReason>> = Lazy::new(|| Mutex::new(SafeModeReason {
    reason:     String::new(),
    entered_at: None,
}));

struct SafeModeReason {
    reason:     String,
    entered_at: Option<u64>,
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Enter safe mode. Heavy services should check is_active() before each tick.
pub fn enter(reason: &str) {
    SAFE_MODE_ACTIVE.store(true, Ordering::SeqCst);
    SAFE_MODE_ENTRIES.fetch_add(1, Ordering::Relaxed);
    let mut s = STATE.lock().unwrap();
    s.reason     = reason.to_string();
    s.entered_at = Some(ts_now());
    crate::preferences_runtime::set_bool("safe_mode", true);
}

/// Exit safe mode and resume normal operation.
pub fn exit() {
    SAFE_MODE_ACTIVE.store(false, Ordering::SeqCst);
    SAFE_MODE_EXITS.fetch_add(1, Ordering::Relaxed);
    let mut s = STATE.lock().unwrap();
    s.reason     = String::new();
    s.entered_at = None;
    crate::preferences_runtime::set_bool("safe_mode", false);
}

pub fn is_active() -> bool { SAFE_MODE_ACTIVE.load(Ordering::Relaxed) }

/// Check if a service is allowed to run in the current mode.
pub fn is_service_allowed(service: &str) -> bool {
    if !is_active() { return true; }
    SAFE_MODE_SERVICES.iter().any(|s| *s == service)
}

pub fn snapshot() -> SafeModeState {
    let s = STATE.lock().unwrap();
    SafeModeState {
        active:            SAFE_MODE_ACTIVE.load(Ordering::Relaxed),
        entered_at:        s.entered_at,
        reason:            s.reason.clone(),
        allowed_services:  SAFE_MODE_SERVICES.iter().map(|s| s.to_string()).collect(),
        disabled_services: DISABLED_IN_SAFE_MODE.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn entries() -> u64 { SAFE_MODE_ENTRIES.load(Ordering::Relaxed) }
pub fn exits()   -> u64 { SAFE_MODE_EXITS.load(Ordering::Relaxed) }

/// Auto-enter safe mode if the runtime hardening detects too many failures.
pub fn auto_enter_if_unstable() {
    // Only trigger on currently disabled modules, not total crash count.
    // Total crashes accumulates during tests and would cause false triggers.
    let disabled = crate::runtime_hardening::modules_disabled();
    if disabled >= 3 {
        if !is_active() {
            enter("auto: too many modules disabled");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;

    // Serialize all safe_mode tests — they share global AtomicBool state.
    static TEST_LOCK: Lazy<std::sync::Mutex<()>> = Lazy::new(|| std::sync::Mutex::new(()));

    #[test]
    fn not_active_by_default() {
        let _g = TEST_LOCK.lock().unwrap();
        exit();
        assert!(!is_active());
    }

    #[test]
    fn enter_and_exit() {
        let _g = TEST_LOCK.lock().unwrap();
        enter("test");
        assert!(is_active(), "safe mode should be active after enter()");
        exit();
        assert!(!is_active(), "safe mode should be inactive after exit()");
    }

    #[test]
    fn allowed_service_in_safe_mode() {
        assert!(SAFE_MODE_SERVICES.contains(&"stt"));
        assert!(!SAFE_MODE_SERVICES.contains(&"world_simulation_runtime"));
    }

    #[test]
    fn all_services_allowed_when_not_safe_mode() {
        let _g = TEST_LOCK.lock().unwrap();
        exit();
        assert!(is_service_allowed("world_simulation_runtime"));
    }

    #[test]
    fn snapshot_reflects_state() {
        let _g = TEST_LOCK.lock().unwrap();
        enter("unit test");
        let s = snapshot();
        assert!(s.active);
        assert!(!s.allowed_services.is_empty());
        exit();
    }
}
