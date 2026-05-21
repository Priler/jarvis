//! Production runtime mode escalation.
//!
//! A 4-level mode system that coordinates long-run stability behaviour.
//! Distinct from the hot-path `watchdog::DEGRADED_MODE` — this layer operates
//! on a slower timescale (seconds, not frames) and governs autonomous recovery.
//!
//! Mode ordering: NORMAL → DEGRADED → SAFE → RECOVERY
//! Escalation goes up freely; de-escalation requires an explicit recovery event.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::SystemTime;

// ── Mode definition ───────────────────────────────────────────────────────────

/// Four-level production runtime mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[repr(u8)]
pub enum ProductionMode {
    /// All subsystems healthy.  Normal operation.
    Normal = 0,
    /// One or more subsystems degraded.  Increased suppression, watchdog active.
    Degraded = 1,
    /// Aggressive suppression.  Wake threshold raised to MAX.  No new features.
    Safe = 2,
    /// Self-healing in progress.  Voice suspended until recovery completes.
    Recovery = 3,
}

impl ProductionMode {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Degraded,
            2 => Self::Safe,
            3 => Self::Recovery,
            _ => Self::Normal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal   => "NORMAL",
            Self::Degraded => "DEGRADED",
            Self::Safe     => "SAFE",
            Self::Recovery => "RECOVERY",
        }
    }
}

impl std::fmt::Display for ProductionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Global state ──────────────────────────────────────────────────────────────

static CURRENT_MODE: AtomicU8 = AtomicU8::new(0);
static MODE_ENTERED_MS: AtomicU64 = AtomicU64::new(0);
static MODE_TRANSITIONS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Current production mode.
pub fn current() -> ProductionMode {
    ProductionMode::from_u8(CURRENT_MODE.load(Ordering::Relaxed))
}

/// Seconds spent in the current mode.
pub fn mode_age_s() -> u64 {
    let entered = MODE_ENTERED_MS.load(Ordering::Relaxed);
    if entered == 0 { return 0; }
    now_ms().saturating_sub(entered) / 1000
}

/// Total mode transitions since startup.
pub fn transition_count() -> u64 {
    MODE_TRANSITIONS.load(Ordering::Relaxed)
}

/// Escalate to `mode` if it is higher than the current mode.
/// Returns `true` if a transition occurred.
///
/// **Safety:** Transitions are monotonically upward during escalation.
/// De-escalation only via `try_recover`.
pub fn escalate(mode: ProductionMode, reason: &str) -> bool {
    let current = current();
    if mode <= current {
        return false;
    }
    commit_transition(mode, reason);
    true
}

/// Attempt to de-escalate to `Normal`.
/// Succeeds only from `Degraded` or `Safe`.
/// From `Recovery`, the recovery engine must clear the mode explicitly.
/// Returns `true` if de-escalation occurred.
pub fn try_recover(reason: &str) -> bool {
    let current = current();
    match current {
        ProductionMode::Degraded | ProductionMode::Safe => {
            commit_transition(ProductionMode::Normal, reason);
            true
        }
        ProductionMode::Recovery => {
            // Recovery is cleared only by the recovery engine after subsystem confirmation.
            info!("[MODES] Recovery mode: de-escalation requires explicit recovery engine clearance");
            false
        }
        ProductionMode::Normal => false,
    }
}

/// Force clear Recovery mode to Normal (only for recovery engine use).
pub fn clear_recovery(reason: &str) {
    if current() == ProductionMode::Recovery {
        commit_transition(ProductionMode::Normal, reason);
    }
}

fn commit_transition(mode: ProductionMode, reason: &str) {
    let prev = current();
    CURRENT_MODE.store(mode as u8, Ordering::Relaxed);
    MODE_ENTERED_MS.store(now_ms(), Ordering::Relaxed);
    MODE_TRANSITIONS.fetch_add(1, Ordering::Relaxed);
    info!(
        "[MODES] {} → {} reason={} transitions={}",
        prev, mode, reason, MODE_TRANSITIONS.load(Ordering::Relaxed)
    );
    write_mode_event(prev, mode, reason);
}

fn write_mode_event(from: ProductionMode, to: ProductionMode, reason: &str) {
    let ts = now_ms();
    let reason_esc = reason.replace('"', "\\\"");
    let line = format!(
        "{{\"ts\":{},\"from\":\"{}\",\"to\":\"{}\",\"reason\":\"{}\"}}",
        ts, from.as_str(), to.as_str(), reason_esc
    );
    if let Some(dir) = jarvis_core::APP_LOG_DIR.get() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(dir.join("mode_transitions.jsonl"))
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_ordering_correct() {
        assert!(ProductionMode::Normal   < ProductionMode::Degraded);
        assert!(ProductionMode::Degraded < ProductionMode::Safe);
        assert!(ProductionMode::Safe     < ProductionMode::Recovery);
    }

    #[test]
    fn from_u8_roundtrip() {
        for v in 0u8..4 {
            assert_eq!(ProductionMode::from_u8(v) as u8, v);
        }
        assert_eq!(ProductionMode::from_u8(99), ProductionMode::Normal);
    }
}
