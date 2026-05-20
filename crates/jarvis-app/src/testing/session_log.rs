#![allow(dead_code)]

//! Session-level event journal.
//!
//! The `SessionJournal` is a single-threaded in-memory store.  The harness
//! drains the `mpsc::Receiver` into the journal just before producing the
//! final report — no background thread needed.

use std::collections::VecDeque;

use super::ValidationEvent;

const MAX_EVENTS: usize = 50_000;

// ── Journal ───────────────────────────────────────────────────────────────────

pub struct SessionJournal {
    events: VecDeque<ValidationEvent>,

    // Pre-computed counters updated on every `record()` call.
    pub wake_opens: u32,
    pub wake_closes: u32,
    pub command_opens: u32,
    pub command_closes: u32,
    pub illegal_transitions: u32,
    /// RecognizerFed{recognizer:"speech", in_state:"VoiceActive"} count.
    pub speech_reco_in_voice_active: u32,
    /// RecognizerFed{recognizer:"speech", in_state:"CommandMode"} count.
    pub speech_reco_in_command_mode: u32,
    /// RecognizerFed{recognizer:"speech"} in any state other than VoiceActive/CommandMode.
    pub speech_reco_other_state: u32,
    /// WakeSessionClose{clean:false} count.
    pub dirty_closes: u32,
    /// Consecutive wake-open timestamps for debounce checking.
    wake_open_ts: Vec<u64>,
    /// SpeakingGateSet events for gate-stuck analysis.
    gate_set_ts: Vec<u64>,
    /// SpeakingGateCleared events.
    gate_clear_ts: Vec<u64>,
    /// IPC events received.
    pub ipc_events: Vec<(&'static str, u64)>,
    /// RecognizerReset{recognizer:"speech"} after WakeSessionClose.
    pub speech_resets_after_close: u32,
    /// RecognizerReset{recognizer:"wake"} count (total, for A013).
    pub wake_reco_resets: u32,
    /// Count of StateTransition{to: "Cooldown"} — every wake session close
    /// should be preceded by at least one Cooldown entry (A015 informational).
    pub cooldown_entries: u32,
    /// Count of CommandSessionOpen events that duplicate the text of the
    /// immediately preceding command in the same wake session (A014 guard).
    pub duplicate_commands: u32,
    /// (text, wake_sid) of the last CommandSessionOpen — used for duplicate detection.
    last_command_info: Option<(String, u64)>,
    /// Snapshot: last WakeSessionClose was clean or dirty.
    last_close_clean: Option<bool>,
    /// Rustpotter detection scores for all confirmed wake events.
    /// Used for threshold calibration analysis.
    pub wake_scores: Vec<f32>,
}

impl SessionJournal {
    pub fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(1024),
            wake_opens: 0,
            wake_closes: 0,
            command_opens: 0,
            command_closes: 0,
            illegal_transitions: 0,
            speech_reco_in_voice_active: 0,
            speech_reco_in_command_mode: 0,
            speech_reco_other_state: 0,
            dirty_closes: 0,
            wake_open_ts: Vec::new(),
            gate_set_ts: Vec::new(),
            gate_clear_ts: Vec::new(),
            ipc_events: Vec::new(),
            speech_resets_after_close: 0,
            wake_reco_resets: 0,
            cooldown_entries: 0,
            duplicate_commands: 0,
            last_command_info: None,
            last_close_clean: None,
            wake_scores: Vec::new(),
        }
    }

    pub fn record(&mut self, event: ValidationEvent) {
        // Update counters.
        match &event {
            ValidationEvent::StateTransition { legal: false, .. } => {
                self.illegal_transitions += 1;
            }
            ValidationEvent::WakeSessionOpen { ts, .. } => {
                self.wake_opens += 1;
                self.wake_open_ts.push(*ts);
            }
            ValidationEvent::WakeSessionClose { clean, ts: _, .. } => {
                self.wake_closes += 1;
                self.last_close_clean = Some(*clean);
                if !clean {
                    self.dirty_closes += 1;
                }
                // reset the "resets after close" accumulator
                self.speech_resets_after_close = 0;
            }
            ValidationEvent::CommandSessionOpen { text, wake_sid, .. } => {
                self.command_opens += 1;
                // Detect same text repeated in the same wake session (P0-1 regression guard).
                if let Some((ref prev_text, prev_wsid)) = self.last_command_info {
                    if prev_wsid == *wake_sid && prev_text.as_str() == text.as_str() {
                        self.duplicate_commands += 1;
                    }
                }
                self.last_command_info = Some((text.clone(), *wake_sid));
            }
            ValidationEvent::CommandSessionClose { .. } => {
                self.command_closes += 1;
            }
            ValidationEvent::RecognizerFed { recognizer: "speech", in_state: "VoiceActive", .. } => {
                self.speech_reco_in_voice_active += 1;
            }
            ValidationEvent::RecognizerFed { recognizer: "speech", in_state: "CommandMode", .. } => {
                self.speech_reco_in_command_mode += 1;
            }
            ValidationEvent::RecognizerFed { recognizer: "speech", .. } => {
                // Catches feeds in any state other than VoiceActive and CommandMode.
                self.speech_reco_other_state += 1;
            }
            ValidationEvent::RecognizerReset { recognizer: "speech", .. } => {
                self.speech_resets_after_close += 1;
            }
            ValidationEvent::RecognizerReset { recognizer: "wake", .. } => {
                self.wake_reco_resets += 1;
            }
            ValidationEvent::StateTransition { to: "Cooldown", .. } => {
                self.cooldown_entries += 1;
            }
            ValidationEvent::SpeakingGateSet { ts, .. } => {
                self.gate_set_ts.push(*ts);
            }
            ValidationEvent::SpeakingGateCleared { ts, .. } => {
                self.gate_clear_ts.push(*ts);
            }
            ValidationEvent::IpcEvent { tag, ts } => {
                self.ipc_events.push((tag, *ts));
            }
            ValidationEvent::WakeScore { score, .. } => {
                self.wake_scores.push(*score);
            }
            _ => {}
        }

        // Cap total events.
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// All recorded events in insertion order.
    pub fn events(&self) -> &VecDeque<ValidationEvent> {
        &self.events
    }

    /// Returns all StateTransition events flagged as illegal.
    pub fn illegal_transition_events(&self) -> Vec<&ValidationEvent> {
        self.events
            .iter()
            .filter(|e| matches!(e, ValidationEvent::StateTransition { legal: false, .. }))
            .collect()
    }

    /// Returns paired (open_ts, close_ts) for wake sessions, where close_ts
    /// is None if no matching close was found (orphaned session).
    pub fn wake_session_pairs(&self) -> Vec<(u64, Option<u64>)> {
        let mut opens: Vec<(u64, u64)> = Vec::new(); // (session_id, ts)
        let mut closes: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();

        for ev in &self.events {
            match ev {
                ValidationEvent::WakeSessionOpen { session_id, ts } => {
                    opens.push((*session_id, *ts));
                }
                ValidationEvent::WakeSessionClose { session_id, ts, .. } => {
                    closes.insert(*session_id, *ts);
                }
                _ => {}
            }
        }

        opens
            .into_iter()
            .map(|(sid, open_ts)| (open_ts, closes.get(&sid).copied()))
            .collect()
    }

    /// Returns paired (open_ts, close_ts) for command sessions.
    pub fn command_session_pairs(&self) -> Vec<(u64, Option<u64>)> {
        let mut opens: Vec<(u64, u64)> = Vec::new();
        let mut closes: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();

        for ev in &self.events {
            match ev {
                ValidationEvent::CommandSessionOpen { session_id, ts, .. } => {
                    opens.push((*session_id, *ts));
                }
                ValidationEvent::CommandSessionClose { session_id, ts, .. } => {
                    closes.insert(*session_id, *ts);
                }
                _ => {}
            }
        }

        opens
            .into_iter()
            .map(|(sid, open_ts)| (open_ts, closes.get(&sid).copied()))
            .collect()
    }

    /// Minimum gap between consecutive WakeSessionOpen timestamps (ms).
    /// Returns None if fewer than 2 wake sessions occurred.
    pub fn min_wake_open_gap_ms(&self) -> Option<u64> {
        if self.wake_open_ts.len() < 2 {
            return None;
        }
        let mut min_gap = u64::MAX;
        for window in self.wake_open_ts.windows(2) {
            let gap = window[1].saturating_sub(window[0]);
            if gap < min_gap {
                min_gap = gap;
            }
        }
        Some(min_gap)
    }

    /// Export the full event list as a JSON string.
    pub fn to_json(&self) -> String {
        let events: Vec<&ValidationEvent> = self.events.iter().collect();
        serde_json::to_string_pretty(&events).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
    }

    /// Export events as JSONL — one compact JSON object per line.
    pub fn to_jsonl(&self) -> String {
        self.events
            .iter()
            .filter_map(|ev| serde_json::to_string(ev).ok())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Derive latency statistics from the event timeline.
    pub fn compute_latency(&self) -> LatencyStats {
        let mut wake_det: Vec<u64> = Vec::new();
        let mut stt_lats: Vec<u64> = Vec::new();
        let mut pipe_lats: Vec<u64> = Vec::new();

        let mut voice_active_ts: Option<u64> = None;
        let mut wake_open_ts: Option<u64> = None;

        for ev in &self.events {
            match ev {
                super::ValidationEvent::StateTransition { to: "VoiceActive", ts, .. } => {
                    voice_active_ts = Some(*ts);
                }
                super::ValidationEvent::WakeSessionOpen { ts, .. } => {
                    if let Some(va_ts) = voice_active_ts {
                        wake_det.push(ts.saturating_sub(va_ts));
                    }
                    wake_open_ts = Some(*ts);
                }
                super::ValidationEvent::CommandSessionOpen { ts, .. } => {
                    if let Some(wo_ts) = wake_open_ts {
                        stt_lats.push(ts.saturating_sub(wo_ts));
                    }
                    if let Some(va_ts) = voice_active_ts {
                        pipe_lats.push(ts.saturating_sub(va_ts));
                    }
                }
                super::ValidationEvent::WakeSessionClose { .. } => {
                    wake_open_ts = None;
                    voice_active_ts = None;
                }
                _ => {}
            }
        }

        fn avg(v: &[u64]) -> Option<u64> {
            if v.is_empty() { None } else { Some(v.iter().sum::<u64>() / v.len() as u64) }
        }
        fn p95(v: &mut Vec<u64>) -> Option<u64> {
            if v.is_empty() { return None; }
            v.sort_unstable();
            Some(v[((v.len() as f64 * 0.95).floor() as usize).min(v.len() - 1)])
        }

        let note = if stt_lats.is_empty() {
            "No commands in run — stt_latency N/A. In accelerated mode values are CPU time, not audio time.".to_string()
        } else {
            "In accelerated mode values reflect CPU processing time, not audio latency.".to_string()
        };

        let mut wd = wake_det.clone();
        let mut sl = stt_lats.clone();
        let mut pl = pipe_lats.clone();

        LatencyStats {
            wake_detection_avg_ms: avg(&wake_det),
            wake_detection_p95_ms: p95(&mut wd),
            stt_avg_ms: avg(&stt_lats),
            stt_p95_ms: p95(&mut sl),
            pipeline_avg_ms: avg(&pipe_lats),
            pipeline_p95_ms: p95(&mut pl),
            wake_detection_samples: wake_det,
            stt_samples: stt_lats,
            pipeline_samples: pipe_lats,
            note,
        }
    }
}

// ── Latency statistics ────────────────────────────────────────────────────────

#[derive(Default, serde::Serialize)]
pub struct LatencyStats {
    /// Per-session latency: StateTransition{to:"VoiceActive"} → WakeSessionOpen (ms).
    pub wake_detection_samples: Vec<u64>,
    /// Per-session latency: WakeSessionOpen → CommandSessionOpen (ms).
    pub stt_samples: Vec<u64>,
    /// Per-session latency: VoiceActive → CommandSessionOpen (ms).
    pub pipeline_samples: Vec<u64>,

    pub wake_detection_avg_ms: Option<u64>,
    pub wake_detection_p95_ms: Option<u64>,
    pub stt_avg_ms: Option<u64>,
    pub stt_p95_ms: Option<u64>,
    pub pipeline_avg_ms: Option<u64>,
    pub pipeline_p95_ms: Option<u64>,

    pub note: String,
}
