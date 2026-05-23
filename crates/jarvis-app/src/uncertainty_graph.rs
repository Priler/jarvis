//! Uncertainty graph — connects uncertain beliefs and cognitive components,
//! propagates confidence, and tracks probabilistic dependencies.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_NODES:        usize = 300;
const MAX_EDGES:        usize = 1000;
const STABLE_THRESHOLD: f32   = 0.50;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Node / Edge ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncertaintyNode {
    pub label:       String,
    pub confidence:  f32,
    pub uncertainty: f32,   // 1 - confidence
    pub is_stable:   bool,
    pub ts_ms:       u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncertaintyEdge {
    pub from:   String,
    pub to:     String,
    pub weight: f32,   // propagation strength 0–1
}

// ── State ─────────────────────────────────────────────────────────────────────

struct UGraph {
    nodes: Vec<UncertaintyNode>,
    edges: Vec<UncertaintyEdge>,
}

static GRAPH: Lazy<Mutex<UGraph>> = Lazy::new(|| Mutex::new(UGraph {
    nodes: Vec::new(),
    edges: Vec::new(),
}));

// ── API ───────────────────────────────────────────────────────────────────────

pub fn upsert_node(label: impl Into<String>, confidence: f32) {
    let label      = label.into();
    let confidence = confidence.clamp(0.0, 1.0);
    let now        = ts_now();
    let mut g = GRAPH.lock().unwrap();
    if let Some(n) = g.nodes.iter_mut().find(|n| n.label == label) {
        n.confidence  = (n.confidence * 0.60 + confidence * 0.40).clamp(0.0, 1.0);
        n.uncertainty = 1.0 - n.confidence;
        n.is_stable   = n.confidence >= STABLE_THRESHOLD;
        n.ts_ms       = now;
        return;
    }
    if g.nodes.len() >= MAX_NODES { g.nodes.remove(0); }
    g.nodes.push(UncertaintyNode {
        label,
        confidence,
        uncertainty: 1.0 - confidence,
        is_stable:   confidence >= STABLE_THRESHOLD,
        ts_ms:       now,
    });
}

pub fn add_dependency(from: impl Into<String>, to: impl Into<String>, weight: f32) {
    let mut g = GRAPH.lock().unwrap();
    if g.edges.len() >= MAX_EDGES { g.edges.remove(0); }
    g.edges.push(UncertaintyEdge {
        from:   from.into(),
        to:     to.into(),
        weight: weight.clamp(0.0, 1.0),
    });
}

/// One pass of confidence propagation through dependency edges.
pub fn propagate(iters: usize) {
    for _ in 0..iters {
        let mut g    = GRAPH.lock().unwrap();
        let snap: Vec<(String, f32)> = g.nodes.iter()
            .map(|n| (n.label.clone(), n.confidence))
            .collect();
        let edges: Vec<(String, String, f32)> = g.edges.iter()
            .map(|e| (e.from.clone(), e.to.clone(), e.weight))
            .collect();
        for (from, to, w) in &edges {
            let src = snap.iter().find(|(l, _)| l == from).map(|(_, c)| *c).unwrap_or(0.5);
            if let Some(n) = g.nodes.iter_mut().find(|n| &n.label == to) {
                let delta     = (src - n.confidence) * w * 0.10;
                n.confidence  = (n.confidence + delta).clamp(0.0, 1.0);
                n.uncertainty = 1.0 - n.confidence;
                n.is_stable   = n.confidence >= STABLE_THRESHOLD;
            }
        }
    }
}

pub fn avg_uncertainty() -> f32 {
    let g = GRAPH.lock().unwrap();
    if g.nodes.is_empty() { return 0.30; }
    g.nodes.iter().map(|n| n.uncertainty).sum::<f32>() / g.nodes.len() as f32
}

pub fn avg_confidence() -> f32 { 1.0 - avg_uncertainty() }

pub fn unstable_nodes() -> Vec<String> {
    GRAPH.lock().unwrap().nodes.iter()
        .filter(|n| !n.is_stable)
        .map(|n| n.label.clone())
        .collect()
}

pub fn node_count() -> usize { GRAPH.lock().unwrap().nodes.len() }
pub fn edge_count() -> usize { GRAPH.lock().unwrap().edges.len() }
pub fn all_nodes()  -> Vec<UncertaintyNode> { GRAPH.lock().unwrap().nodes.clone() }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_propagate() {
        upsert_node("src_ug_ph22", 0.8);
        upsert_node("dst_ug_ph22", 0.3);
        add_dependency("src_ug_ph22", "dst_ug_ph22", 0.5);
        propagate(3);
        let u = avg_uncertainty();
        assert!(u >= 0.0 && u <= 1.0);
    }

    #[test]
    fn unstable_below_threshold() {
        upsert_node("low_conf_ph22", 0.2);
        let unstable = unstable_nodes();
        assert!(unstable.iter().any(|l| l == "low_conf_ph22"));
    }
}
