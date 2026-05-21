//! Active observer — detects changes in the desktop environment each tick.
//!
//! Compares the current desktop snapshot against the previous one and produces
//! a `ChangeSet` describing what changed (windows, focus, dialogs, etc.).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static OBSERVATIONS:       AtomicU64 = AtomicU64::new(0);
pub static CHANGES_DETECTED:   AtomicU64 = AtomicU64::new(0);
pub static NO_CHANGES_DETECTED: AtomicU64 = AtomicU64::new(0);

// ── Change kinds ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ChangeKind {
    WindowOpened      { title: String },
    WindowClosed      { title: String },
    FocusChanged      { from: Option<String>, to: Option<String> },
    DialogAppeared    { hint: String },
    DialogDismissed,
    ActiveAppChanged  { from: Option<String>, to: Option<String> },
    WindowCountChanged { prev: usize, curr: usize },
    ScreenLockChanged { locked: bool },
}

// ── Change set ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangeSet {
    pub ts_ms:   u64,
    pub changes: Vec<ChangeKind>,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn has_dialog_change(&self) -> bool {
        self.changes.iter().any(|c| matches!(c,
            ChangeKind::DialogAppeared { .. } | ChangeKind::DialogDismissed))
    }

    pub fn has_focus_change(&self) -> bool {
        self.changes.iter().any(|c| matches!(c, ChangeKind::FocusChanged { .. }))
    }

    pub fn has_app_change(&self) -> bool {
        self.changes.iter().any(|c| matches!(c, ChangeKind::ActiveAppChanged { .. }))
    }
}

// ── Observation result ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ObservationResult {
    pub ts_ms:      u64,
    pub changes:    ChangeSet,
    pub window_count: usize,
    pub focused_title: Option<String>,
    pub has_modal:  bool,
    pub env_state:  String,
}

impl ObservationResult {
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }
}

// ── Previous observation cache ────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct PreviousState {
    window_count:  usize,
    focused_title: Option<String>,
    has_modal:     bool,
    active_app:    Option<String>,
}

static PREV: Lazy<Mutex<PreviousState>> = Lazy::new(|| Mutex::new(PreviousState::default()));

// ── Active observer ───────────────────────────────────────────────────────────

pub struct ActiveObserver;

impl ActiveObserver {
    pub fn observe() -> ObservationResult {
        OBSERVATIONS.fetch_add(1, Ordering::Relaxed);

        use crate::world_state;
        use crate::environment_reasoner::EnvironmentReasoner;

        let window_count   = world_state::with_state(|s| {
            s.snapshot.as_ref().map(|snap| snap.window_count()).unwrap_or(0)
        });
        let focused_str    = world_state::focused_window_title();
        let focused_title  = if focused_str.is_empty() { None } else { Some(focused_str) };
        let has_modal      = world_state::has_blocking_modal();
        let active_app     = world_state::with_state(|s| s.active_app.clone());
        let reasoning      = EnvironmentReasoner::reason();
        let env_state      = format!("{:?}", reasoning.state);

        let mut changes = Vec::new();

        if let Ok(mut prev) = PREV.lock() {
            // Window count delta
            if prev.window_count != window_count {
                changes.push(ChangeKind::WindowCountChanged {
                    prev: prev.window_count,
                    curr: window_count,
                });
            }

            // Focus delta
            if prev.focused_title != focused_title {
                changes.push(ChangeKind::FocusChanged {
                    from: prev.focused_title.clone(),
                    to:   focused_title.clone(),
                });
            }

            // Modal delta
            if !prev.has_modal && has_modal {
                changes.push(ChangeKind::DialogAppeared { hint: "unknown".to_string() });
            } else if prev.has_modal && !has_modal {
                changes.push(ChangeKind::DialogDismissed);
            }

            // Active app delta
            if prev.active_app != active_app {
                changes.push(ChangeKind::ActiveAppChanged {
                    from: prev.active_app.clone(),
                    to:   active_app.clone(),
                });
            }

            // Update cached state
            prev.window_count  = window_count;
            prev.focused_title = focused_title.clone();
            prev.has_modal     = has_modal;
            prev.active_app    = active_app;
        }

        let change_set = ChangeSet { ts_ms: ts_now(), changes };

        if change_set.is_empty() {
            NO_CHANGES_DETECTED.fetch_add(1, Ordering::Relaxed);
        } else {
            CHANGES_DETECTED.fetch_add(1, Ordering::Relaxed);
        }

        ObservationResult {
            ts_ms: ts_now(),
            changes: change_set,
            window_count,
            focused_title,
            has_modal,
            env_state,
        }
    }

    pub fn reset_baseline() {
        if let Ok(mut prev) = PREV.lock() {
            *prev = PreviousState::default();
        }
    }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_runs_without_panic() {
        ActiveObserver::reset_baseline();
        let result = ActiveObserver::observe();
        assert!(result.ts_ms > 0);
    }

    #[test]
    fn change_set_empty_check() {
        let cs = ChangeSet { ts_ms: ts_now(), changes: vec![] };
        assert!(cs.is_empty());
    }

    #[test]
    fn change_set_dialog_detection() {
        let cs = ChangeSet {
            ts_ms: ts_now(),
            changes: vec![ChangeKind::DialogAppeared { hint: "confirm".into() }],
        };
        assert!(cs.has_dialog_change());
        assert!(!cs.has_focus_change());
    }

    #[test]
    fn change_set_focus_detection() {
        let cs = ChangeSet {
            ts_ms: ts_now(),
            changes: vec![ChangeKind::FocusChanged { from: None, to: Some("editor".into()) }],
        };
        assert!(cs.has_focus_change());
    }

    #[test]
    fn observations_counter_increments() {
        let before = OBSERVATIONS.load(Ordering::Relaxed);
        ActiveObserver::observe();
        assert!(OBSERVATIONS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn observation_has_env_state() {
        ActiveObserver::reset_baseline();
        let result = ActiveObserver::observe();
        assert!(!result.env_state.is_empty());
    }
}
