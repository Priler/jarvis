//! Window graph — hierarchy and relationship model for open windows.
//!
//! Tracks open windows, their parent/child relationships, overlays,
//! focus state, and visibility.  Used by the world-state runtime and
//! environment reasoner.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub static WINDOW_GRAPH_UPDATES: AtomicU64 = AtomicU64::new(0);

// ── Window status ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum WindowStatus {
    Open,
    Minimised,
    Hidden,
    Maximised,
    Closing,
    Crashed,
}

impl WindowStatus {
    pub fn is_visible(&self) -> bool {
        matches!(self, WindowStatus::Open | WindowStatus::Maximised)
    }
}

// ── Window relation ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum WindowRelation {
    /// Top-level application window.
    Root,
    /// Child window inside a parent.
    ChildOf(String),
    /// Modal dialog blocking parent.
    ModalOver(String),
    /// Notification/overlay floating above parent.
    OverlayOf(String),
}

// ── Window node ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowNode {
    pub id:           String,
    pub title:        String,
    pub process_name: String,
    pub status:       WindowStatus,
    pub relation:     WindowRelation,
    pub is_focused:   bool,
    pub z_order:      u32,
    pub child_ids:    Vec<String>,
}

impl WindowNode {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        process: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            process_name: process.into(),
            status: WindowStatus::Open,
            relation: WindowRelation::Root,
            is_focused: false,
            z_order: 0,
            child_ids: Vec::new(),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.status.is_visible()
    }

    pub fn is_blocking_modal(&self) -> bool {
        matches!(self.relation, WindowRelation::ModalOver(_)) && self.is_visible()
    }
}

// ── Window graph ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowGraph {
    nodes: HashMap<String, WindowNode>,
}

impl WindowGraph {
    pub fn new() -> Self {
        Self { nodes: HashMap::new() }
    }

    pub fn add(&mut self, node: WindowNode) {
        self.nodes.insert(node.id.clone(), node);
        WINDOW_GRAPH_UPDATES.fetch_add(1, Ordering::Relaxed);
    }

    pub fn remove(&mut self, id: &str) {
        self.nodes.remove(id);
    }

    pub fn get(&self, id: &str) -> Option<&WindowNode> {
        self.nodes.get(id)
    }

    pub fn focused_window(&self) -> Option<&WindowNode> {
        self.nodes.values().find(|n| n.is_focused)
    }

    pub fn visible_windows(&self) -> Vec<&WindowNode> {
        let mut v: Vec<&WindowNode> = self.nodes.values()
            .filter(|n| n.is_visible())
            .collect();
        v.sort_by(|a, b| b.z_order.cmp(&a.z_order));
        v
    }

    pub fn blocking_modals(&self) -> Vec<&WindowNode> {
        self.nodes.values().filter(|n| n.is_blocking_modal()).collect()
    }

    pub fn count(&self) -> usize {
        self.nodes.len()
    }

    pub fn has_blocking_modal(&self) -> bool {
        self.nodes.values().any(|n| n.is_blocking_modal())
    }

    pub fn windows_for_process(&self, process: &str) -> Vec<&WindowNode> {
        let p = process.to_lowercase();
        self.nodes.values()
            .filter(|n| n.process_name.to_lowercase().contains(&p))
            .collect()
    }

    pub fn find_by_title_contains(&self, fragment: &str) -> Option<&WindowNode> {
        let f = fragment.to_lowercase();
        self.nodes.values()
            .find(|n| n.title.to_lowercase().contains(&f))
    }
}

impl Default for WindowGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, title: &str, process: &str) -> WindowNode {
        WindowNode::new(id, title, process)
    }

    #[test]
    fn empty_graph_has_no_windows() {
        let g = WindowGraph::new();
        assert_eq!(g.count(), 0);
        assert!(!g.has_blocking_modal());
    }

    #[test]
    fn add_and_find_window() {
        let mut g = WindowGraph::new();
        g.add(make_node("w1", "Visual Studio Code", "code"));
        assert_eq!(g.count(), 1);
        assert!(g.find_by_title_contains("Visual Studio").is_some());
    }

    #[test]
    fn modal_blocking_detected() {
        let mut g = WindowGraph::new();
        let mut modal = make_node("m1", "Save changes?", "notepad");
        modal.relation = WindowRelation::ModalOver("w1".into());
        modal.status = WindowStatus::Open;
        g.add(modal);
        assert!(g.has_blocking_modal());
        assert_eq!(g.blocking_modals().len(), 1);
    }

    #[test]
    fn minimised_window_not_visible() {
        let mut n = make_node("w1", "Notepad", "notepad");
        n.status = WindowStatus::Minimised;
        assert!(!n.is_visible());
    }

    #[test]
    fn focused_window_returned() {
        let mut g = WindowGraph::new();
        let mut n = make_node("w1", "Terminal", "wt");
        n.is_focused = true;
        g.add(n);
        assert!(g.focused_window().is_some());
        assert_eq!(g.focused_window().unwrap().id, "w1");
    }

    #[test]
    fn windows_for_process_filter() {
        let mut g = WindowGraph::new();
        g.add(make_node("w1", "VS Code 1", "code"));
        g.add(make_node("w2", "VS Code 2", "code"));
        g.add(make_node("w3", "Notepad", "notepad"));
        assert_eq!(g.windows_for_process("code").len(), 2);
        assert_eq!(g.windows_for_process("notepad").len(), 1);
    }
}

