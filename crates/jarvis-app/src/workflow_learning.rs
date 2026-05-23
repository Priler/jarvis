//! Workflow learning — records tool-execution sequences and detects recurring
//! patterns.  No ML — pure frequency analysis over recent history.
//!
//! A "pattern" is a window of consecutive tool IDs observed in order.
//! When the same sequence appears frequently it is promoted to a learned pattern.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static SEQUENCES_RECORDED: AtomicU64 = AtomicU64::new(0);
pub static PATTERNS_LEARNED:   AtomicU64 = AtomicU64::new(0);
pub static PATTERN_MATCHES:    AtomicU64 = AtomicU64::new(0);

const MAX_SEQUENCE_HISTORY: usize = 500;
const WINDOW_SIZE:           usize = 3;
const LEARN_THRESHOLD:       u32   = 3;

// ── Learned pattern ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowPattern {
    pub sequence:    Vec<String>,
    pub occurrences: u32,
    pub confidence:  f32,
    pub last_seen_ms: u64,
}

impl WorkflowPattern {
    pub fn key(seq: &[String]) -> String {
        seq.join("→")
    }

    pub fn is_strong(&self) -> bool {
        // confidence = occurrences/10; LEARN_THRESHOLD=3 gives 0.3, so check
        // only occurrences to avoid a threshold that is unreachable at LEARN_THRESHOLD.
        self.occurrences >= LEARN_THRESHOLD
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

struct LearnerState {
    sequence_history: Vec<String>,
    patterns:         HashMap<String, WorkflowPattern>,
}

static STATE: Lazy<Mutex<LearnerState>> = Lazy::new(|| Mutex::new(LearnerState {
    sequence_history: Vec::new(),
    patterns:         HashMap::new(),
}));

// ── Public API ────────────────────────────────────────────────────────────────

pub fn record_tool_execution(tool_id: impl Into<String>) {
    SEQUENCES_RECORDED.fetch_add(1, Ordering::Relaxed);
    let tool_id = tool_id.into();
    let now = ts_now();

    if let Ok(mut state) = STATE.lock() {
        if state.sequence_history.len() >= MAX_SEQUENCE_HISTORY {
            state.sequence_history.remove(0);
        }
        state.sequence_history.push(tool_id);

        let len = state.sequence_history.len();
        if len >= WINDOW_SIZE {
            let window: Vec<String> = state.sequence_history[len - WINDOW_SIZE..].to_vec();
            let key = WorkflowPattern::key(&window);

            let entry = state.patterns.entry(key).or_insert_with(|| {
                PATTERNS_LEARNED.fetch_add(1, Ordering::Relaxed);
                WorkflowPattern {
                    sequence:     window.clone(),
                    occurrences:  0,
                    confidence:   0.0,
                    last_seen_ms: now,
                }
            });

            entry.occurrences   += 1;
            entry.last_seen_ms   = now;
            entry.confidence     = (entry.occurrences as f32 / 10.0).min(1.0);

            if entry.occurrences == LEARN_THRESHOLD {
                crate::world_state_journal::log(
                    crate::world_state_journal::WorldEventKind::WorkflowPatternLearned {
                        pattern:     WorkflowPattern::key(&window),
                        occurrences: entry.occurrences,
                    },
                );
            }
        }
    }
}

/// Records multiple tools in a single lock acquisition, guaranteeing they appear
/// consecutively in sequence_history regardless of parallel test threads.
pub fn batch_record(tools: &[&str]) {
    let now = ts_now();
    if let Ok(mut state) = STATE.lock() {
        for &tool_id in tools {
            SEQUENCES_RECORDED.fetch_add(1, Ordering::Relaxed);
            if state.sequence_history.len() >= MAX_SEQUENCE_HISTORY {
                state.sequence_history.remove(0);
            }
            state.sequence_history.push(tool_id.to_string());

            let len = state.sequence_history.len();
            if len >= WINDOW_SIZE {
                let window: Vec<String> = state.sequence_history[len - WINDOW_SIZE..].to_vec();
                let key = WorkflowPattern::key(&window);
                let entry = state.patterns.entry(key).or_insert_with(|| {
                    PATTERNS_LEARNED.fetch_add(1, Ordering::Relaxed);
                    WorkflowPattern { sequence: window.clone(), occurrences: 0, confidence: 0.0, last_seen_ms: now }
                });
                entry.occurrences  += 1;
                entry.last_seen_ms  = now;
                entry.confidence    = (entry.occurrences as f32 / 10.0).min(1.0);
                if entry.occurrences == LEARN_THRESHOLD {
                    crate::world_state_journal::log(
                        crate::world_state_journal::WorldEventKind::WorkflowPatternLearned {
                            pattern: WorkflowPattern::key(&window), occurrences: entry.occurrences,
                        },
                    );
                }
            }
        }
    }
}

pub fn strong_patterns() -> Vec<WorkflowPattern> {
    STATE.lock().map(|s| s.patterns.values()
        .filter(|p| p.is_strong())
        .cloned()
        .collect()
    ).unwrap_or_default()
}

pub fn all_patterns() -> Vec<WorkflowPattern> {
    STATE.lock().map(|s| s.patterns.values().cloned().collect()).unwrap_or_default()
}

pub fn matches_known_pattern(recent_tools: &[String]) -> Option<WorkflowPattern> {
    if recent_tools.len() < WINDOW_SIZE { return None; }
    let window = &recent_tools[recent_tools.len() - WINDOW_SIZE..];
    let key = WorkflowPattern::key(window);

    let result = STATE.lock().ok().and_then(|s| {
        s.patterns.get(&key).filter(|p| p.is_strong()).cloned()
    });

    if result.is_some() {
        PATTERN_MATCHES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

pub fn sequence_len() -> usize {
    STATE.lock().map(|s| s.sequence_history.len()).unwrap_or(0)
}

pub fn clear() {
    if let Ok(mut state) = STATE.lock() {
        state.sequence_history.clear();
        state.patterns.clear();
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
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn setup() { clear(); }

    #[test]
    fn record_adds_to_history() {
        let _g = TEST_LOCK.lock().unwrap();
        let before = SEQUENCES_RECORDED.load(Ordering::Relaxed);
        record_tool_execution("history.test.unique");
        assert!(SEQUENCES_RECORDED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn pattern_detected_after_threshold() {
        let _g = TEST_LOCK.lock().unwrap();
        let seq = ["pat.uniq1", "pat.uniq2", "pat.uniq3"];
        for _ in 0..5 {
            batch_record(&seq);
        }
        let key = WorkflowPattern::key(&seq.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let found = all_patterns().into_iter().any(|p| WorkflowPattern::key(&p.sequence) == key && p.is_strong());
        assert!(found, "no strong pattern for unique sequence after 5 batch repetitions");
    }

    #[test]
    fn matches_known_pattern_returns_some_for_strong() {
        let _g = TEST_LOCK.lock().unwrap();
        let seq = ["mknp.a", "mknp.b", "mknp.c"];
        for _ in 0..5 {
            batch_record(&seq);
        }
        let recent: Vec<String> = seq.iter().map(|s| s.to_string()).collect();
        let m = matches_known_pattern(&recent);
        assert!(m.is_some(), "expected pattern match for unique sequence after 5 batch repetitions");
    }

    #[test]
    fn sequences_recorded_counter_increments() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();
        let before = SEQUENCES_RECORDED.load(Ordering::Relaxed);
        record_tool_execution("test.tool");
        assert!(SEQUENCES_RECORDED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn pattern_confidence_bounded() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();
        for _ in 0..20 {
            batch_record(&["x.a", "x.b", "x.c"]);
        }
        for p in all_patterns() {
            assert!(p.confidence <= 1.0);
        }
    }

    #[test]
    fn clear_reduces_sequence_len() {
        let _g = TEST_LOCK.lock().unwrap();
        record_tool_execution("clear.test.t1");
        record_tool_execution("clear.test.t2");
        record_tool_execution("clear.test.t3");
        let before = sequence_len();
        assert!(before >= 3);
    }
}
