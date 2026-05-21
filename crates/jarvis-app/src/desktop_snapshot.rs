//! Desktop snapshot — point-in-time capture of the full desktop environment.
//!
//! Combines screen pixel data, window graph, and UI state into a single
//! immutable record.  Created by the world-state runtime and consumed by
//! verifiers and the environment reasoner.

use std::time::SystemTime;

use crate::screen_capture::CaptureResult;
use crate::ui_state::UiState;
use crate::window_graph::WindowGraph;

// ── Snapshot kind ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SnapshotKind {
    /// Complete desktop capture + all window metadata.
    Full,
    /// Active window only.
    ActiveWindow,
    /// Lightweight metadata-only (no pixel data).
    MetadataOnly,
}

// ── Desktop snapshot ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct DesktopSnapshot {
    pub ts_ms:         u64,
    pub kind:          SnapshotKind,
    pub capture:       Option<CaptureResult>,
    pub window_graph:  WindowGraph,
    pub ui_state:      UiState,
    pub screen_width:  u32,
    pub screen_height: u32,
}

impl DesktopSnapshot {
    pub fn new_stub(kind: SnapshotKind) -> Self {
        Self {
            ts_ms: now_ms(),
            kind,
            capture: None,
            window_graph: WindowGraph::new(),
            ui_state: UiState::new(),
            screen_width: 1920,
            screen_height: 1080,
        }
    }

    pub fn with_capture(mut self, capture: CaptureResult) -> Self {
        self.screen_width  = capture.width;
        self.screen_height = capture.height;
        self.capture = Some(capture);
        self
    }

    pub fn with_window_graph(mut self, graph: WindowGraph) -> Self {
        self.window_graph = graph;
        self
    }

    pub fn with_ui_state(mut self, state: UiState) -> Self {
        self.ui_state = state;
        self
    }

    pub fn has_pixel_data(&self) -> bool {
        self.capture.as_ref().map(|c| c.has_data()).unwrap_or(false)
    }

    pub fn window_count(&self) -> usize {
        self.window_graph.count()
    }

    pub fn has_blocking_modal(&self) -> bool {
        self.window_graph.has_blocking_modal() || self.ui_state.has_blocking_dialog()
    }

    pub fn age_ms(&self) -> u64 {
        now_ms().saturating_sub(self.ts_ms)
    }

    /// True when the snapshot is older than `max_age_ms`.
    pub fn is_stale(&self, max_age_ms: u64) -> bool {
        self.age_ms() > max_age_ms
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_graph::WindowNode;

    #[test]
    fn stub_snapshot_has_no_pixel_data() {
        let s = DesktopSnapshot::new_stub(SnapshotKind::MetadataOnly);
        assert!(!s.has_pixel_data());
        assert_eq!(s.window_count(), 0);
    }

    #[test]
    fn snapshot_window_count_reflects_graph() {
        let mut graph = WindowGraph::new();
        graph.add(WindowNode::new("w1", "Code", "code"));
        graph.add(WindowNode::new("w2", "Terminal", "wt"));
        let s = DesktopSnapshot::new_stub(SnapshotKind::Full)
            .with_window_graph(graph);
        assert_eq!(s.window_count(), 2);
    }

    #[test]
    fn blocking_modal_from_window_graph_detected() {
        use crate::window_graph::{WindowRelation, WindowStatus};
        let mut graph = WindowGraph::new();
        let mut modal = WindowNode::new("m1", "Confirm", "app");
        modal.relation = WindowRelation::ModalOver("w1".into());
        modal.status = WindowStatus::Open;
        graph.add(modal);
        let s = DesktopSnapshot::new_stub(SnapshotKind::Full)
            .with_window_graph(graph);
        assert!(s.has_blocking_modal());
    }

    #[test]
    fn snapshot_ts_is_set() {
        let s = DesktopSnapshot::new_stub(SnapshotKind::Full);
        assert!(s.ts_ms > 0);
    }

    #[test]
    fn new_snapshot_is_not_stale() {
        let s = DesktopSnapshot::new_stub(SnapshotKind::Full);
        assert!(!s.is_stale(5_000));
    }
}
