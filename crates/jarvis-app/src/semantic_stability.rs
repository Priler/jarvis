//! Semantic stability engine — detects semantic drift, inference instability,
//! symbolic recursion risk, contradiction amplification, and semantic collapse.

use std::sync::Mutex;
use once_cell::sync::Lazy;

const MAX_HISTORY:       usize = 100;
const DRIFT_THRESHOLD:   f32   = 0.20;
const COLLAPSE_THRESHOLD: f32  = 0.15;

// ── SemanticStabilityReport ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticStabilityReport {
    pub is_stable:           bool,
    pub instability_score:   f32,     // 0–1
    pub semantic_drift:      f32,     // change in inference_quality vs previous cycle
    pub contradiction_rate:  f32,     // contradictions / reasoning_cycles
    pub has_collapse_risk:   bool,
    pub has_recursion_risk:  bool,
    pub recommendation:      String,
    pub ts_ms:               u64,
}

impl SemanticStabilityReport {
    pub fn is_critical(&self) -> bool { self.has_collapse_risk || self.instability_score > 0.80 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct StabilityState {
    history:        Vec<SemanticStabilityReport>,
    prev_quality:   Option<f32>,
}

static STATE: Lazy<Mutex<StabilityState>> = Lazy::new(|| Mutex::new(StabilityState {
    history:      Vec::new(),
    prev_quality: None,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn check() -> SemanticStabilityReport {
    // Current inference quality
    let chains = crate::symbolic_inference::reliable_chains();
    let current_quality = if chains.is_empty() { 0.5 }
        else { chains.iter().map(|c| c.confidence).sum::<f32>() / chains.len() as f32 };

    // Semantic drift: change from previous cycle
    let semantic_drift = STATE.lock().map(|s| {
        s.prev_quality.map(|prev| (current_quality - prev).abs()).unwrap_or(0.0)
    }).unwrap_or(0.0);

    // Contradiction rate: contradictions / cycles
    let total_detected = crate::semantic_contradictions::CONTRADICTIONS_DETECTED
        .load(std::sync::atomic::Ordering::Relaxed);
    let cycles = crate::semantic_reasoner::REASONING_CYCLES
        .load(std::sync::atomic::Ordering::Relaxed).max(1);
    let contradiction_rate = (total_detected as f32 / cycles as f32).min(1.0);

    // Recursion risk: max chain depth
    let max_depth = chains.iter().map(|c| c.depth).max().unwrap_or(0);
    let has_recursion_risk = max_depth >= crate::symbolic_safety::MAX_INFERENCE_DEPTH - 1;

    // Collapse risk: quality below threshold
    let has_collapse_risk = current_quality < COLLAPSE_THRESHOLD;

    // Instability score
    let instability_score = (contradiction_rate * 0.4
        + semantic_drift / (DRIFT_THRESHOLD.max(0.01)) * 0.3
        + (if has_recursion_risk { 0.2 } else { 0.0 })
        + (if has_collapse_risk  { 0.1 } else { 0.0 }))
        .clamp(0.0, 1.0);

    let is_stable = instability_score < 0.50 && !has_collapse_risk;

    let recommendation = if has_collapse_risk {
        "restart_semantic_graph_population".to_string()
    } else if has_recursion_risk {
        "reduce_inference_chain_depth".to_string()
    } else if contradiction_rate > 0.5 {
        "resolve_contradictions_before_inference".to_string()
    } else if semantic_drift > DRIFT_THRESHOLD {
        "stabilize_concept_observations".to_string()
    } else {
        "continue_semantic_reasoning".to_string()
    };

    // Log if unstable
    if !is_stable {
        crate::symbolic_observability::log(
            crate::symbolic_observability::SymbolicEvent::StabilityCheck {
                is_stable: false,
                reason: recommendation.clone(),
            }
        );
    }

    let report = SemanticStabilityReport {
        is_stable, instability_score, semantic_drift, contradiction_rate,
        has_collapse_risk, has_recursion_risk, recommendation, ts_ms: ts_now(),
    };

    if let Ok(mut s) = STATE.lock() {
        if s.history.len() >= MAX_HISTORY { s.history.remove(0); }
        s.history.push(report.clone());
        s.prev_quality = Some(current_quality);
    }
    report
}

pub fn latest() -> Option<SemanticStabilityReport> {
    STATE.lock().ok().and_then(|s| s.history.last().cloned())
}

pub fn is_stable() -> bool {
    latest().map(|r| r.is_stable).unwrap_or(true)
}
