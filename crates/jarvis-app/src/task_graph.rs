//! Multi-step task graph for planning and execution.
//!
//! A `TaskGraph` is a DAG of `TaskNode`s where each node represents a single
//! tool invocation.  The planner builds the graph; the executor walks it in
//! topological order.
//!
//! Guarantees:
//!   - Topological sort fails fast if a cycle is detected.
//!   - No node executes before its dependencies complete successfully.
//!   - Max 16 nodes per graph — protects against runaway LLM plans.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

pub static GRAPHS_CREATED:  AtomicU64 = AtomicU64::new(0);
pub static NODES_COMPLETED: AtomicU64 = AtomicU64::new(0);
pub static NODES_FAILED:    AtomicU64 = AtomicU64::new(0);

pub const MAX_GRAPH_NODES: usize = 16;

// ── Node status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed { reason: String },
    Skipped,
}

// ── Task node ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskNode {
    pub id:          String,
    pub description: String,
    pub tool_id:     String,
    pub arg:         String,
    /// IDs of nodes that must complete before this one runs.
    pub depends_on:  Vec<String>,
    pub status:      NodeStatus,
    pub retry_count: u8,
}

impl TaskNode {
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        tool_id: impl Into<String>,
        arg: impl Into<String>,
    ) -> Self {
        Self {
            id:          id.into(),
            description: description.into(),
            tool_id:     tool_id.into(),
            arg:         arg.into(),
            depends_on:  Vec::new(),
            status:      NodeStatus::Pending,
            retry_count: 0,
        }
    }

    pub fn with_deps(mut self, deps: Vec<String>) -> Self {
        self.depends_on = deps;
        self
    }

    pub fn is_ready(&self, completed: &HashSet<String>) -> bool {
        self.status == NodeStatus::Pending
            && self.depends_on.iter().all(|d| completed.contains(d))
    }
}

// ── Task graph ────────────────────────────────────────────────────────────────

pub struct TaskGraph {
    nodes: HashMap<String, TaskNode>,
    /// IDs in insertion order for deterministic iteration.
    order: Vec<String>,
}

impl TaskGraph {
    pub fn new() -> Self {
        GRAPHS_CREATED.fetch_add(1, Ordering::Relaxed);
        Self { nodes: HashMap::new(), order: Vec::new() }
    }

    /// Add a node. Returns Err if the graph already has MAX_GRAPH_NODES nodes.
    pub fn add(&mut self, node: TaskNode) -> Result<(), String> {
        if self.nodes.len() >= MAX_GRAPH_NODES {
            return Err(format!(
                "task graph exceeds max {} nodes — possible runaway plan",
                MAX_GRAPH_NODES
            ));
        }
        let id = node.id.clone();
        self.order.push(id.clone());
        self.nodes.insert(id, node);
        Ok(())
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Topological sort (Kahn's algorithm).  Returns Err if a cycle is found.
    pub fn topological_order(&self) -> Result<Vec<String>, String> {
        // Build in-degree map.
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        for id in &self.order {
            in_degree.entry(id).or_insert(0);
        }
        for id in &self.order {
            let node = &self.nodes[id];
            for dep in &node.depends_on {
                *in_degree.entry(dep.as_str()).or_insert(0) += 0;
                if let Some(cnt) = in_degree.get_mut(id.as_str()) {
                    *cnt += 1;
                }
            }
        }

        // Re-compute properly.
        let mut in_deg: HashMap<String, usize> = self.order.iter()
            .map(|id| (id.clone(), 0))
            .collect();
        for id in &self.order {
            let node = &self.nodes[id];
            for dep in &node.depends_on {
                if let Some(cnt) = in_deg.get_mut(id) {
                    *cnt += 1;
                }
                let _ = dep; // dep is a predecessor, not counted here
            }
        }

        // Kahn: queue nodes with in_degree 0.
        // We count edges as: node N has in_degree = len(N.depends_on).
        let mut deg: HashMap<String, usize> = self.order.iter()
            .map(|id| (id.clone(), self.nodes[id].depends_on.len()))
            .collect();

        let mut queue: VecDeque<String> = deg.iter()
            .filter(|(_, &d)| d == 0)
            .map(|(id, _)| id.clone())
            .collect();

        // Stable order: sort queue entries.
        let mut queue_vec: Vec<String> = queue.drain(..).collect();
        queue_vec.sort();
        queue.extend(queue_vec);

        // Build adjacency: dep → [nodes that depend on dep].
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        for id in &self.order {
            let node = &self.nodes[id];
            for dep in &node.depends_on {
                dependents.entry(dep.clone()).or_default().push(id.clone());
            }
        }

        let mut result = Vec::new();
        while let Some(id) = queue.pop_front() {
            result.push(id.clone());
            if let Some(children) = dependents.get(&id) {
                let mut sorted = children.clone();
                sorted.sort();
                for child in sorted {
                    if let Some(d) = deg.get_mut(&child) {
                        *d -= 1;
                        if *d == 0 {
                            queue.push_back(child);
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            Err("task graph has a cycle".to_string())
        } else {
            Ok(result)
        }
    }

    /// Mark a node completed and update observability counter.
    pub fn mark_completed(&mut self, id: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = NodeStatus::Completed;
            NODES_COMPLETED.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Mark a node failed.
    pub fn mark_failed(&mut self, id: &str, reason: impl Into<String>) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = NodeStatus::Failed { reason: reason.into() };
            NODES_FAILED.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// All completed node IDs.
    pub fn completed_ids(&self) -> HashSet<String> {
        self.nodes.iter()
            .filter(|(_, n)| n.status == NodeStatus::Completed)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// True when all nodes are in a terminal state (Completed/Failed/Skipped).
    pub fn is_done(&self) -> bool {
        self.nodes.values().all(|n| matches!(
            n.status,
            NodeStatus::Completed | NodeStatus::Failed { .. } | NodeStatus::Skipped
        ))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str) -> TaskNode {
        TaskNode::new(id, "desc", "app.open", "arg")
    }

    #[test]
    fn empty_graph_topo_sort_succeeds() {
        let g = TaskGraph::new();
        assert!(g.topological_order().unwrap().is_empty());
    }

    #[test]
    fn single_node_topo_sort() {
        let mut g = TaskGraph::new();
        g.add(make_node("a")).unwrap();
        let order = g.topological_order().unwrap();
        assert_eq!(order, vec!["a"]);
    }

    #[test]
    fn linear_chain_topo_sort() {
        let mut g = TaskGraph::new();
        g.add(make_node("a")).unwrap();
        g.add(TaskNode::new("b", "desc", "app.open", "arg").with_deps(vec!["a".into()])).unwrap();
        g.add(TaskNode::new("c", "desc", "app.open", "arg").with_deps(vec!["b".into()])).unwrap();
        let order = g.topological_order().unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn max_nodes_limit_enforced() {
        let mut g = TaskGraph::new();
        for i in 0..MAX_GRAPH_NODES {
            g.add(make_node(&format!("n{}", i))).unwrap();
        }
        let err = g.add(make_node("overflow"));
        assert!(err.is_err());
    }

    #[test]
    fn mark_completed_sets_status() {
        let mut g = TaskGraph::new();
        g.add(make_node("a")).unwrap();
        g.mark_completed("a");
        assert!(g.completed_ids().contains("a"));
        assert!(g.is_done());
    }

    #[test]
    fn graphs_created_counter_increments() {
        let before = GRAPHS_CREATED.load(Ordering::Relaxed);
        let _g = TaskGraph::new();
        assert!(GRAPHS_CREATED.load(Ordering::Relaxed) > before);
    }
}
