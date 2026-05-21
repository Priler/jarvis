//! World-state runtime — aggregated model of the current desktop environment.
//!
//! Maintains the latest `DesktopSnapshot` and exposes getters for the
//! environment reasoner, multimodal verifier, and planner.
//!
//! Update cadence: driven by the executor before/after each tool call.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use parking_lot::RwLock;

use crate::desktop_snapshot::{DesktopSnapshot, SnapshotKind};
use crate::ui_state::UiState;
use crate::window_graph::WindowGraph;

pub static WORLD_STATE_SNAPSHOTS: AtomicU64 = AtomicU64::new(0);
pub static WORLD_STATE_UPDATES:   AtomicU64 = AtomicU64::new(0);
pub static WORLD_STATE_STALE:     AtomicU64 = AtomicU64::new(0);

const STALE_THRESHOLD_MS: u64 = 5_000;

// ── Application state ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AppPresence {
    Running { window_count: usize },
    Minimised,
    NotRunning,
    Crashed,
}

// ── World state ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldState {
    pub snapshot:        Option<DesktopSnapshot>,
    pub known_processes: Vec<String>,
    pub active_app:      Option<String>,
    pub workspace_ready: bool,
}

impl WorldState {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            known_processes: Vec::new(),
            active_app: None,
            workspace_ready: false,
        }
    }

    pub fn update_snapshot(&mut self, snap: DesktopSnapshot) {
        self.active_app = snap.window_graph.focused_window()
            .map(|w| w.process_name.clone());
        self.snapshot = Some(snap);
        WORLD_STATE_UPDATES.fetch_add(1, Ordering::Relaxed);
    }

    pub fn current_snapshot(&self) -> Option<&DesktopSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn is_stale(&self) -> bool {
        match &self.snapshot {
            None => true,
            Some(s) => s.is_stale(STALE_THRESHOLD_MS),
        }
    }

    pub fn open_window_count(&self) -> usize {
        self.snapshot.as_ref().map(|s| s.window_count()).unwrap_or(0)
    }

    pub fn has_blocking_modal(&self) -> bool {
        self.snapshot.as_ref().map(|s| s.has_blocking_modal()).unwrap_or(false)
    }

    pub fn focused_window_title(&self) -> &str {
        self.snapshot.as_ref()
            .and_then(|s| s.window_graph.focused_window())
            .map(|w| w.title.as_str())
            .unwrap_or("")
    }

    pub fn app_is_running(&self, process: &str) -> bool {
        match &self.snapshot {
            None => false,
            Some(s) => !s.window_graph.windows_for_process(process).is_empty(),
        }
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Runtime singleton ─────────────────────────────────────────────────────────

static STATE: Lazy<RwLock<WorldState>> = Lazy::new(|| RwLock::new(WorldState::new()));

pub fn update(snapshot: DesktopSnapshot) {
    WORLD_STATE_SNAPSHOTS.fetch_add(1, Ordering::Relaxed);
    STATE.write().update_snapshot(snapshot);
}

pub fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&WorldState) -> R,
{
    let guard = STATE.read();
    f(&*guard)
}

pub fn is_stale() -> bool {
    let stale = STATE.read().is_stale();
    if stale { WORLD_STATE_STALE.fetch_add(1, Ordering::Relaxed); }
    stale
}

pub fn app_is_running(process: &str) -> bool {
    STATE.read().app_is_running(process)
}

pub fn has_blocking_modal() -> bool {
    STATE.read().has_blocking_modal()
}

pub fn focused_window_title() -> String {
    STATE.read().focused_window_title().to_string()
}

/// Take a fresh metadata-only snapshot and commit it to world state.
pub fn refresh_stub() {
    let snap = DesktopSnapshot::new_stub(SnapshotKind::MetadataOnly);
    update(snap);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_graph::WindowNode;

    fn snap_with_window(title: &str, process: &str) -> DesktopSnapshot {
        let mut graph = WindowGraph::new();
        let mut node = WindowNode::new("w1", title, process);
        node.is_focused = true;
        graph.add(node);
        DesktopSnapshot::new_stub(SnapshotKind::Full)
            .with_window_graph(graph)
    }

    #[test]
    fn new_world_state_is_stale() {
        let s = WorldState::new();
        assert!(s.is_stale());
        assert_eq!(s.open_window_count(), 0);
    }

    #[test]
    fn update_snapshot_sets_active_app() {
        let mut ws = WorldState::new();
        ws.update_snapshot(snap_with_window("VS Code", "code"));
        assert_eq!(ws.active_app.as_deref(), Some("code"));
    }

    #[test]
    fn app_is_running_check() {
        let mut ws = WorldState::new();
        ws.update_snapshot(snap_with_window("Code", "code"));
        assert!(ws.app_is_running("code"));
        assert!(!ws.app_is_running("notepad"));
    }

    #[test]
    fn focused_window_title_from_snapshot() {
        let mut ws = WorldState::new();
        ws.update_snapshot(snap_with_window("My Project — VS Code", "code"));
        assert!(ws.focused_window_title().contains("VS Code"));
    }

    #[test]
    fn world_state_snapshots_counter_increments() {
        let before = WORLD_STATE_SNAPSHOTS.load(Ordering::Relaxed);
        refresh_stub();
        assert!(WORLD_STATE_SNAPSHOTS.load(Ordering::Relaxed) > before);
    }
}
