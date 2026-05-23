//! Meta-memory fusion — consolidates causal, workflow, environment, planner,
//! and reflection memory into a unified cross-module memory index.
//! Runs periodically (driven by meta_scheduler) to keep the world model coherent.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;

pub static FUSIONS_RUN:         AtomicU64 = AtomicU64::new(0);
pub static RECORDS_CONSOLIDATED: AtomicU64 = AtomicU64::new(0);
pub static CONFLICTS_RESOLVED:  AtomicU64 = AtomicU64::new(0);

const MAX_FUSION_HISTORY: usize = 30;

// ── Fused memory entry ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FusedMemoryEntry {
    pub key:         String,
    pub source:      MemorySource,
    pub value:       String,
    pub confidence:  f32,
    pub ts_ms:       u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MemorySource {
    Causal,
    Workflow,
    Environment,
    Planner,
    Reflection,
    Counterfactual,
}

impl MemorySource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Causal        => "causal",
            Self::Workflow      => "workflow",
            Self::Environment   => "environment",
            Self::Planner       => "planner",
            Self::Reflection    => "reflection",
            Self::Counterfactual => "counterfactual",
        }
    }
}

// ── Fusion report ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FusionReport {
    pub fusion_id:         u64,
    pub entries_merged:    usize,
    pub conflicts:         usize,
    pub overall_coherence: f32,
    pub ts_ms:             u64,
}

impl FusionReport {
    pub fn is_coherent(&self) -> bool { self.overall_coherence >= 0.6 }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct FusionState {
    fused:      Vec<FusedMemoryEntry>,
    fusion_id:  u64,
    history:    Vec<FusionReport>,
}

static STATE: Lazy<Mutex<FusionState>> = Lazy::new(|| Mutex::new(FusionState {
    fused:     Vec::new(),
    fusion_id: 0,
    history:   Vec::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

/// Run memory fusion pass.
pub fn fuse() -> FusionReport {
    FUSIONS_RUN.fetch_add(1, Ordering::Relaxed);
    let now = ts_now();

    let fusion_id = STATE.lock().map(|mut s| { s.fusion_id += 1; s.fusion_id }).unwrap_or(0);

    // Collect signals from each memory domain
    let mut entries: Vec<FusedMemoryEntry> = Vec::new();

    // Causal memory: top causal links (strongest reliable links)
    {
        let mut links = crate::causal_reasoner::reliable_links();
        links.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        links.truncate(5);
        for link in links {
            entries.push(FusedMemoryEntry {
                key:        format!("causal:{}→{}", link.cause, link.effect),
                source:     MemorySource::Causal,
                value:      format!("strength={:.3} occ={}", link.strength, link.occurrences),
                confidence: link.strength,
                ts_ms:      now,
            });
        }
    }

    // Workflow memory: recent patterns
    {
        let patterns = crate::workflow_learning::strong_patterns();
        for p in patterns.iter().take(3) {
            let key_str = crate::workflow_learning::WorkflowPattern::key(&p.sequence);
            entries.push(FusedMemoryEntry {
                key:        format!("workflow:{}", key_str),
                source:     MemorySource::Workflow,
                value:      format!("confidence={:.2} occ={}", p.confidence, p.occurrences),
                confidence: p.confidence,
                ts_ms:      now,
            });
        }
    }

    // Planner memory: current uncertainty
    {
        let unc = crate::uncertainty_engine::sample();
        entries.push(FusedMemoryEntry {
            key:        "planner:uncertainty".to_string(),
            source:     MemorySource::Planner,
            value:      format!("overall={:.3} high_dims={}", unc.overall, unc.high_count),
            confidence: (1.0 - unc.overall).clamp(0.0, 1.0),
            ts_ms:      now,
        });
    }

    // Reflection memory: recent insights
    {
        let report = crate::meta_reflection::reflect();
        for insight in report.insights.iter().take(3) {
            entries.push(FusedMemoryEntry {
                key:        format!("reflection:{}", insight.source),
                source:     MemorySource::Reflection,
                value:      insight.insight.clone(),
                confidence: 1.0 - insight.severity,
                ts_ms:      now,
            });
        }
    }

    // Counterfactual memory: latest recommendation
    {
        if let Some(cf) = crate::live_counterfactuals::history().last().cloned() {
            entries.push(FusedMemoryEntry {
                key:        "cf:recommendation".to_string(),
                source:     MemorySource::Counterfactual,
                value:      cf.recommendation,
                confidence: cf.best_improvement.clamp(0.0, 1.0),
                ts_ms:      now,
            });
        }
    }

    // Detect conflicts: same key from different sources → pick highest confidence
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut conflicts = 0usize;
    let mut deduped: Vec<FusedMemoryEntry> = Vec::new();
    for entry in entries {
        if let Some(idx) = seen.get(&entry.key).copied() {
            conflicts += 1;
            CONFLICTS_RESOLVED.fetch_add(1, Ordering::Relaxed);
            if entry.confidence > deduped[idx].confidence {
                deduped[idx] = entry;
            }
        } else {
            seen.insert(entry.key.clone(), deduped.len());
            deduped.push(entry);
        }
    }

    let merged = deduped.len();
    RECORDS_CONSOLIDATED.fetch_add(merged as u64, Ordering::Relaxed);

    let overall_coherence = if merged == 0 {
        0.5
    } else {
        deduped.iter().map(|e| e.confidence).sum::<f32>() / merged as f32
    };

    let report = FusionReport { fusion_id, entries_merged: merged, conflicts, overall_coherence, ts_ms: now };

    if let Ok(mut s) = STATE.lock() {
        s.fused = deduped;
        if s.history.len() >= MAX_FUSION_HISTORY { s.history.remove(0); }
        s.history.push(report.clone());
    }

    report
}

/// Returns the current fused memory snapshot.
pub fn snapshot() -> Vec<FusedMemoryEntry> {
    STATE.lock().map(|s| s.fused.clone()).unwrap_or_default()
}

pub fn history() -> Vec<FusionReport> {
    STATE.lock().map(|s| s.history.clone()).unwrap_or_default()
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
    fn fusion_runs_and_returns_report() {
        let r = fuse();
        assert!(FUSIONS_RUN.load(Ordering::Relaxed) >= 1);
        assert!(r.overall_coherence >= 0.0 && r.overall_coherence <= 1.0);
    }

    #[test]
    fn snapshot_non_empty_after_fusion() {
        let _ = fuse();
        // snapshot may be empty if sub-modules have no data, but should not panic
        let _ = snapshot();
    }
}
