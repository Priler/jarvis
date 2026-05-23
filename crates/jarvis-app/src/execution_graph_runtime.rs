//! Execution graph runtime view — visualization metadata over TaskGraph state.
//!
//! Distinct from `cognitive/execution_graph.rs` (which provides the execution engine).
//! This module provides a serializable graph view for display/debugging:
//!   - Node status at a point in time
//!   - Edge relationships
//!   - Overall plan progress summary
//!
//! The view is a snapshot — it does not modify the TaskGraph.

use crate::task_graph::{NodeStatus, TaskGraph, TaskNode};

// ── Node view ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeView {
    pub id:      String,
    pub tool_id: String,
    pub status:  NodeViewStatus,
    pub deps:    Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum NodeViewStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Rollback,
}

impl NodeViewStatus {
    fn from_node_status(status: &NodeStatus) -> Self {
        match status {
            NodeStatus::Pending    => NodeViewStatus::Pending,
            NodeStatus::Running    => NodeViewStatus::Running,
            NodeStatus::Completed  => NodeViewStatus::Completed,
            NodeStatus::Failed {..}=> NodeViewStatus::Failed,
            NodeStatus::Skipped    => NodeViewStatus::Skipped,
        }
    }
}

// ── Graph view ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphView {
    pub nodes:          Vec<NodeView>,
    pub total:          usize,
    pub completed:      usize,
    pub failed:         usize,
    pub pending:        usize,
    pub progress_pct:   f32,
}

impl GraphView {
    pub fn is_complete(&self) -> bool {
        self.completed + self.failed == self.total
    }

    pub fn all_succeeded(&self) -> bool {
        self.completed == self.total && self.failed == 0
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Build a `GraphView` snapshot from a `TaskGraph`.
///
/// Traverses in topological order if possible; falls back to insertion order
/// if the graph has a cycle (which should not happen after planner validation).
pub fn build_view(graph: &TaskGraph) -> GraphView {
    // We can't access graph nodes directly (they're in a HashMap), so we use
    // the public interface: topological_order + completed_ids.
    let topo = graph.topological_order().unwrap_or_default();
    let completed_set = graph.completed_ids();
    let is_done = graph.is_done();

    // We can only derive limited status from the public API.
    // Completed is known; Failed/Pending are inferred.
    let mut nodes = Vec::new();
    let mut n_completed = 0usize;
    let mut n_pending = 0usize;

    for id in &topo {
        let status = if completed_set.contains(id) {
            n_completed += 1;
            NodeViewStatus::Completed
        } else {
            n_pending += 1;
            NodeViewStatus::Pending
        };

        nodes.push(NodeView {
            id:      id.clone(),
            tool_id: id.clone(), // tool_id not public from TaskGraph; use id as proxy
            status,
            deps:    vec![],
        });
    }

    let total = topo.len();
    let progress_pct = if total == 0 { 100.0 } else { (n_completed as f32 / total as f32) * 100.0 };

    GraphView {
        nodes,
        total,
        completed: n_completed,
        failed:    0,
        pending:   n_pending,
        progress_pct,
    }
}

/// Serialize a `GraphView` to a JSON string for display.
pub fn to_json(view: &GraphView) -> String {
    serde_json::to_string_pretty(view).unwrap_or_else(|_| "{}".to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_graph::{TaskGraph, TaskNode};

    fn make_graph_with_nodes(ids: &[&str]) -> TaskGraph {
        let mut g = TaskGraph::new();
        for id in ids {
            g.add(TaskNode::new(*id, "desc", "app.open", "arg")).unwrap();
        }
        g
    }

    #[test]
    fn empty_graph_view_is_complete() {
        let g = TaskGraph::new();
        let view = build_view(&g);
        assert!(view.is_complete());
        assert_eq!(view.total, 0);
    }

    #[test]
    fn all_pending_nodes_show_pending() {
        let g = make_graph_with_nodes(&["a", "b", "c"]);
        let view = build_view(&g);
        assert_eq!(view.total, 3);
        assert_eq!(view.pending, 3);
        assert_eq!(view.completed, 0);
        assert!(!view.all_succeeded());
    }

    #[test]
    fn completed_nodes_reflected_in_view() {
        let mut g = make_graph_with_nodes(&["a", "b"]);
        g.mark_completed("a");
        let view = build_view(&g);
        assert_eq!(view.completed, 1);
        assert_eq!(view.pending, 1);
    }

    #[test]
    fn progress_pct_is_50_when_half_done() {
        let mut g = make_graph_with_nodes(&["a", "b"]);
        g.mark_completed("a");
        let view = build_view(&g);
        assert!((view.progress_pct - 50.0).abs() < 1.0);
    }

    #[test]
    fn to_json_produces_valid_json() {
        let g = make_graph_with_nodes(&["a"]);
        let view = build_view(&g);
        let json = to_json(&view);
        assert!(json.contains("\"total\""));
    }
}
