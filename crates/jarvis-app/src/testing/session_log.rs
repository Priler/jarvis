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
    /// Snapshot: last WakeSessionClose was clean or dirty.
    last_close_clean: Option<bool>,
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
            dirty_closes: 0,
            wake_open_ts: Vec::new(),
            gate_set_ts: Vec::new(),
            gate_clear_ts: Vec::new(),
            ipc_events: Vec::new(),
            speech_resets_after_close: 0,
            last_close_clean: None,
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
            ValidationEvent::CommandSessionOpen { .. } => {
                self.command_opens += 1;
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
            ValidationEvent::RecognizerReset { recognizer: "speech", .. } => {
                self.speech_resets_after_close += 1;
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
}
