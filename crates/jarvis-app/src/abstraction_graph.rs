//! Abstraction graph — connects concepts, workflows, failures, and strategies
//! via typed edges. Supports hierarchical abstraction, cross-domain links,
//! semantic inheritance, and concept similarity queries.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static NODES_ADDED:    AtomicU64 = AtomicU64::new(0);
pub static EDGES_ADDED:    AtomicU64 = AtomicU64::new(0);
pub static QUERIES_TOTAL:  AtomicU64 = AtomicU64::new(0);

const MAX_NODES: usize = 500;
const MAX_EDGES: usize = 2000;

// ── NodeKind ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    Concept,
    Workflow,
    Failure,
    Strategy,
    Resource,
    Goal,
    Pattern,
}

// ── EdgeKind ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EdgeKind {
    CausalLink,           // A causes B
    SemanticSimilarity,   // A ≈ B structurally
    StrategicTransfer,    // strategy A applies to domain B
    InheritedFrom,        // A is a specialisation of B
    LeadsTo,              // A transitions to B
    Mitigates,            // A reduces/solves B
}

// ── Graph node / edge ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphNode {
    pub id:     u64,
    pub kind:   NodeKind,
    pub label:  String,
    pub weight: f32,    // importance / confidence 0–1
    pub ts_ms:  u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphEdge {
    pub id:     u64,
    pub from:   u64,
    pub to:     u64,
    pub kind:   EdgeKind,
    pub weight: f32,    // strength of the relationship
    pub ts_ms:  u64,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct GraphState {
    nodes:     Vec<GraphNode>,
    edges:     Vec<GraphEdge>,
    node_seq:  u64,
    edge_seq:  u64,
    label_idx: HashMap<String, u64>,   // label → node id
}

static STATE: Lazy<Mutex<GraphState>> = Lazy::new(|| Mutex::new(GraphState {
    nodes:     Vec::new(),
    edges:     Vec::new(),
    node_seq:  0,
    edge_seq:  0,
    label_idx: HashMap::new(),
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Add or reinforce a node. Returns its id.
pub fn add_node(label: impl Into<String>, kind: NodeKind, weight: f32) -> u64 {
    let label = label.into();
    let now = ts_now();

    if let Ok(mut s) = STATE.lock() {
        if let Some(&existing_id) = s.label_idx.get(&label) {
            // Reinforce existing node
            if let Some(n) = s.nodes.iter_mut().find(|n| n.id == existing_id) {
                n.weight = (n.weight + weight).min(1.0);
                n.ts_ms  = now;
            }
            return existing_id;
        }
        if s.nodes.len() >= MAX_NODES { s.nodes.remove(0); }
        s.node_seq += 1;
        let id = s.node_seq;
        s.label_idx.insert(label.clone(), id);
        s.nodes.push(GraphNode { id, kind, label, weight: weight.clamp(0.0, 1.0), ts_ms: now });
        NODES_ADDED.fetch_add(1, Ordering::Relaxed);
        id
    } else {
        0
    }
}

/// Add a directed edge between two nodes by id.
pub fn add_edge(from: u64, to: u64, kind: EdgeKind, weight: f32) {
    if from == to { return; }
    let now = ts_now();
    if let Ok(mut s) = STATE.lock() {
        // Avoid duplicate edges of the same kind
        if s.edges.iter().any(|e| e.from == from && e.to == to && e.kind == kind) {
            // Reinforce instead
            if let Some(e) = s.edges.iter_mut().find(|e| e.from == from && e.to == to && e.kind == kind) {
                e.weight = (e.weight + 0.05).min(1.0);
                e.ts_ms  = now;
            }
            return;
        }
        if s.edges.len() >= MAX_EDGES { s.edges.remove(0); }
        s.edge_seq += 1;
        let edge_id = s.edge_seq;
        s.edges.push(GraphEdge { id: edge_id, from, to, kind, weight: weight.clamp(0.0, 1.0), ts_ms: now });
        EDGES_ADDED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Add edge by label (auto-creates nodes if they don't exist).
pub fn link(from_label: impl Into<String>, from_kind: NodeKind,
            to_label: impl Into<String>,   to_kind:   NodeKind,
            edge_kind: EdgeKind, weight: f32) {
    let from_id = add_node(from_label, from_kind, weight);
    let to_id   = add_node(to_label,   to_kind,   weight);
    add_edge(from_id, to_id, edge_kind, weight);
}

/// Get node id by label.
pub fn node_id(label: &str) -> Option<u64> {
    STATE.lock().ok().and_then(|s| s.label_idx.get(label).copied())
}

/// Get outgoing neighbour node ids from a given node.
pub fn neighbors(node_id: u64) -> Vec<GraphNode> {
    QUERIES_TOTAL.fetch_add(1, Ordering::Relaxed);
    STATE.lock().map(|s| {
        let ids: Vec<u64> = s.edges.iter()
            .filter(|e| e.from == node_id)
            .map(|e| e.to)
            .collect();
        s.nodes.iter().filter(|n| ids.contains(&n.id)).cloned().collect()
    }).unwrap_or_default()
}

/// Find nodes similar to a given node (connected by SemanticSimilarity edges).
pub fn similar_nodes(node_id: u64) -> Vec<GraphNode> {
    QUERIES_TOTAL.fetch_add(1, Ordering::Relaxed);
    STATE.lock().map(|s| {
        let ids: Vec<u64> = s.edges.iter()
            .filter(|e| e.kind == EdgeKind::SemanticSimilarity && (e.from == node_id || e.to == node_id))
            .map(|e| if e.from == node_id { e.to } else { e.from })
            .collect();
        s.nodes.iter().filter(|n| ids.contains(&n.id)).cloned().collect()
    }).unwrap_or_default()
}

/// Find all edges into/out of a node.
pub fn edges_for(node_id: u64) -> Vec<GraphEdge> {
    STATE.lock()
        .map(|s| s.edges.iter().filter(|e| e.from == node_id || e.to == node_id).cloned().collect())
        .unwrap_or_default()
}

pub fn snapshot_nodes() -> Vec<GraphNode> {
    STATE.lock().map(|s| s.nodes.clone()).unwrap_or_default()
}

pub fn snapshot_edges() -> Vec<GraphEdge> {
    STATE.lock().map(|s| s.edges.clone()).unwrap_or_default()
}

/// Count nodes and edges.
pub fn stats() -> (usize, usize) {
    STATE.lock()
        .map(|s| (s.nodes.len(), s.edges.len()))
        .unwrap_or((0, 0))
}
