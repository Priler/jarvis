#![allow(dead_code)]

//! Runtime assertion engine.
//!
//! Each assertion is a deterministic check over the `SessionJournal` and
//! optionally live global atomics.  Every assertion has a stable ID so CI
//! scripts can gate on specific checks by name.

use std::sync::atomic::Ordering;

use super::session_log::SessionJournal;

/// Wake-debounce window from stt_worker.rs (must stay in sync).
const WAKE_DEBOUNCE_MS: u64 = 2500;

// ── Assertion result ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AssertionResult {
    pub id: String,
    pub description: String,
    pub passed: bool,
    pub failures: Vec<String>,
}

impl AssertionResult {
    fn pass(id: &str, description: &str) -> Self {
        Self { id: id.to_string(), description: description.to_string(), passed: true, failures: Vec::new() }
    }

    fn fail(id: &str, description: &str, reason: impl Into<String>) -> Self {
        Self { id: id.to_string(), description: description.to_string(), passed: false, failures: vec![reason.into()] }
    }

    fn with_failures(id: &str, description: &str, failures: Vec<String>) -> Self {
        let passed = failures.is_empty();
        Self { id: id.to_string(), description: description.to_string(), passed, failures }
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub struct AssertionEngine {
    pub results: Vec<AssertionResult>,
}

impl AssertionEngine {
    /// Run all assertions against the completed journal.
    pub fn run_all(journal: &SessionJournal) -> Self {
        let results = vec![
            Self::a001_no_pre_wake_contamination(journal),
            Self::a002_no_illegal_transitions(journal),
            Self::a003_wake_session_balance(journal),
            Self::a004_command_session_balance(journal),
            Self::a005_conversation_depth_reset(journal),
            Self::a006_debounce_respected(journal),
            Self::a007_recognizer_reset_after_wake(journal),
            Self::a008_no_forced_gate_resets(journal),
            Self::a009_no_sessions_in_voice_active(journal),
            Self::a010_ipc_ordering_coherence(journal),
        ];
        Self { results }
    }

    pub fn passed_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    pub fn failed_count(&self) -> usize {
        self.results.iter().filter(|r| !r.passed).count()
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    // ── A001 ─────────────────────────────────────────────────────────────────

    /// SPEECH_RECOGNIZER must not be fed frames while in VoiceActive state.
    ///
    /// Feeds in VoiceActive contaminate the decoder with wake-word audio
    /// before CommandMode begins, causing garbled first transcripts and
    /// silent single-word command drops.  (Audit P0-2)
    fn a001_no_pre_wake_contamination(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A001";
        const DESC: &str = "Speech recognizer not fed during VoiceActive (P0-2: pre-wake contamination)";

        if journal.wake_opens == 0 {
            return AssertionResult::pass(ID, DESC); // no wake ever happened — nothing to check
        }

        if journal.speech_reco_in_voice_active > 0 {
            AssertionResult::fail(
                ID,
                DESC,
                format!(
                    "SPEECH_RECOGNIZER was fed {} time(s) while state=VoiceActive. \
                     Contamination confirmed — first transcript after wake is unreliable. \
                     Fix: remove stt::recognize() call from VoiceActive arm in stt_worker.rs:763.",
                    journal.speech_reco_in_voice_active
                ),
            )
        } else {
            AssertionResult::pass(ID, DESC)
        }
    }

    // ── A002 ─────────────────────────────────────────────────────────────────

    /// All state machine transitions must be in the legal transition table.
    fn a002_no_illegal_transitions(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A002";
        const DESC: &str = "All state transitions are legal";

        if journal.illegal_transitions == 0 {
            return AssertionResult::pass(ID, DESC);
        }

        let details: Vec<String> = journal
            .illegal_transition_events()
            .iter()
            .map(|ev| {
                if let super::ValidationEvent::StateTransition { from, to, reason, wake_sid, .. } = ev {
                    format!("WAKE S:{} {} → {} reason={}", wake_sid, from, to, reason)
                } else {
                    format!("{:?}", ev)
                }
            })
            .collect();

        AssertionResult::with_failures(ID, DESC, details)
    }

    // ── A003 ─────────────────────────────────────────────────────────────────

    /// Every WakeSessionOpen must have a matching WakeSessionClose.
    /// ACTIVE_WAKE_SESSION must be 0 at assertion time.
    fn a003_wake_session_balance(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A003";
        const DESC: &str = "Wake sessions balanced (no session leaks)";

        let mut failures = Vec::new();

        if journal.wake_opens != journal.wake_closes {
            failures.push(format!(
                "opens={} closes={} — {} session(s) not finalized",
                journal.wake_opens,
                journal.wake_closes,
                journal.wake_opens.saturating_sub(journal.wake_closes),
            ));
        }

        // Check live global.
        let active = crate::stt_worker::ACTIVE_WAKE_SESSION.load(Ordering::Acquire);
        if active != 0 {
            failures.push(format!(
                "ACTIVE_WAKE_SESSION={} after run end (expected 0) — zombie session",
                active
            ));
        }

        for (open_ts, close_opt) in journal.wake_session_pairs() {
            if close_opt.is_none() {
                failures.push(format!("WakeSessionOpen at ts={} has no matching Close", open_ts));
            }
        }

        AssertionResult::with_failures(ID, DESC, failures)
    }

    // ── A004 ─────────────────────────────────────────────────────────────────

    /// Every CommandSessionOpen must have a matching CommandSessionClose.
    fn a004_command_session_balance(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A004";
        const DESC: &str = "Command sessions balanced (no orphaned command sessions)";

        let mut failures = Vec::new();

        if journal.command_opens != journal.command_closes {
            failures.push(format!(
                "opens={} closes={} — {} command session(s) never closed",
                journal.command_opens,
                journal.command_closes,
                journal.command_opens.saturating_sub(journal.command_closes),
            ));
        }

        let active_cmd = crate::stt_worker::ACTIVE_COMMAND_SESSION.load(Ordering::Acquire);
        if active_cmd != 0 {
            failures.push(format!(
                "ACTIVE_COMMAND_SESSION={} after run end (expected 0)",
                active_cmd
            ));
        }

        for (open_ts, close_opt) in journal.command_session_pairs() {
            if close_opt.is_none() {
                failures.push(format!(
                    "CommandSessionOpen at ts={} has no matching Close",
                    open_ts
                ));
            }
        }

        AssertionResult::with_failures(ID, DESC, failures)
    }

    // ── A005 ─────────────────────────────────────────────────────────────────

    /// After a timeout (dirty) session, conversation depth must be 0.
    ///
    /// Dirty sessions do not reset VoiceContext under current code.
    /// If depth > 0 after dirty sessions, adaptive silence windows shrink
    /// and subsequent sessions time out prematurely.  (Audit P1-3)
    fn a005_conversation_depth_reset(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A005";
        const DESC: &str = "Conversation depth reset after timeout sessions (P1-3)";

        if journal.dirty_closes == 0 {
            return AssertionResult::pass(ID, DESC);
        }

        // The session ended dirty — check live VOICE_CTX depth.
        let depth = crate::voice_intelligence::VOICE_CTX.lock().conversation_depth;
        if depth > 0 {
            AssertionResult::fail(
                ID,
                DESC,
                format!(
                    "conversation_depth={} after {} dirty close(s). \
                     Should be 0. Adaptive silence window is shorter than intended. \
                     Fix: unconditionally reset VOICE_CTX in finalize_wake().",
                    depth, journal.dirty_closes
                ),
            )
        } else {
            AssertionResult::pass(ID, DESC)
        }
    }

    // ── A006 ─────────────────────────────────────────────────────────────────

    /// Two WakeSessionOpen events must not occur within WAKE_DEBOUNCE_MS.
    fn a006_debounce_respected(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A006";
        const DESC: &str = "Wake debounce respected (min gap between wakes ≥ 2500 ms)";

        if let Some(min_gap) = journal.min_wake_open_gap_ms() {
            if min_gap < WAKE_DEBOUNCE_MS {
                return AssertionResult::fail(
                    ID,
                    DESC,
                    format!(
                        "Min gap between consecutive wakes = {}ms < debounce {}ms. \
                         Debounce guard may have been bypassed.",
                        min_gap, WAKE_DEBOUNCE_MS
                    ),
                );
            }
        }
        AssertionResult::pass(ID, DESC)
    }

    // ── A007 ─────────────────────────────────────────────────────────────────

    /// After every WakeSessionClose, both recognizers must be reset.
    ///
    /// If the speech recognizer is not reset, stale decoder state from the
    /// previous session bleeds into the next.  (Audit LV-4 related)
    fn a007_recognizer_reset_after_wake(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A007";
        const DESC: &str = "Speech recognizer reset after every wake session close";

        // We track speech_resets_after_close which is reset on each WakeSessionClose
        // and incremented on each subsequent RecognizerReset{speech}.
        // At end of run, if we have closes but no trailing reset, fail.
        if journal.wake_closes == 0 {
            return AssertionResult::pass(ID, DESC);
        }

        // Count WakeSessionClose events and RecognizerReset{speech} events, and
        // verify that for every close there's at least one reset following.
        let mut close_count = 0u32;
        let mut pending_reset = false;
        let mut unreseted_closes = 0u32;

        for ev in journal.events() {
            match ev {
                super::ValidationEvent::WakeSessionClose { .. } => {
                    if close_count > 0 && pending_reset {
                        unreseted_closes += 1;
                    }
                    close_count += 1;
                    pending_reset = true;
                }
                super::ValidationEvent::RecognizerReset { recognizer: "speech", .. } => {
                    pending_reset = false;
                }
                _ => {}
            }
        }
        // Check the last close.
        if close_count > 0 && pending_reset {
            unreseted_closes += 1;
        }

        if unreseted_closes > 0 {
            AssertionResult::fail(
                ID,
                DESC,
                format!(
                    "{} wake session close(s) were not followed by a speech recognizer reset.",
                    unreseted_closes
                ),
            )
        } else {
            AssertionResult::pass(ID, DESC)
        }
    }

    // ── A008 ─────────────────────────────────────────────────────────────────

    /// The watchdog should not need to force-clear the speaking gate.
    ///
    /// A forced gate reset indicates the gate was stuck beyond 30 seconds.
    /// This points to a missed `is_speaking()` clearance or an incorrect
    /// duration calculation (Rodio backend returns Duration::ZERO).
    fn a008_no_forced_gate_resets(_journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A008";
        const DESC: &str = "No watchdog-forced speaking gate resets during run";

        let forced = crate::stt_worker::FORCED_GATE_RESETS.load(Ordering::Relaxed);
        if forced > 0 {
            AssertionResult::fail(
                ID,
                DESC,
                format!(
                    "FORCED_GATE_RESETS={} — speaking gate was stuck and had to be force-cleared. \
                     Check audio backend (Rodio returns Duration::ZERO — Audit P0-1).",
                    forced
                ),
            )
        } else {
            AssertionResult::pass(ID, DESC)
        }
    }

    // ── A009 ─────────────────────────────────────────────────────────────────

    /// RecognizerFed events in VoiceActive imply the speech decoder is
    /// contaminated.  This assertion counts contamination events per wake
    /// session and fails if the ratio is high enough to cause misrecognitions.
    fn a009_no_sessions_in_voice_active(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A009";
        const DESC: &str = "Speech recognizer feeds in VoiceActive (pre-wake contamination) per session";

        if journal.wake_opens == 0 {
            return AssertionResult::pass(ID, DESC);
        }

        // Average frames fed per session.
        let avg = journal.speech_reco_in_voice_active as f64 / journal.wake_opens as f64;
        if avg > 5.0 {
            AssertionResult::fail(
                ID,
                DESC,
                format!(
                    "avg {:.1} speech-reco frames fed per VoiceActive session ({} feeds / {} wakes). \
                     High contamination rate — single-word commands will silently fail.",
                    avg, journal.speech_reco_in_voice_active, journal.wake_opens
                ),
            )
        } else {
            AssertionResult::pass(ID, DESC)
        }
    }

    // ── A010 ─────────────────────────────────────────────────────────────────

    /// Basic IPC event ordering: WakeWordDetected must precede Listening
    /// which must precede Idle for each session.
    fn a010_ipc_ordering_coherence(journal: &SessionJournal) -> AssertionResult {
        const ID: &str = "A010";
        const DESC: &str = "IPC event ordering: WakeWordDetected → Listening → Idle per session";

        if journal.ipc_events.is_empty() {
            // IPC subscription not active — skip, not a failure.
            return AssertionResult::pass(ID, DESC);
        }

        let mut seen_wake = false;
        let mut seen_listen = false;
        let mut failures = Vec::new();
        let mut session = 0u32;

        for (tag, _ts) in &journal.ipc_events {
            match *tag {
                "wake_word_detected" => {
                    session += 1;
                    seen_wake = true;
                    seen_listen = false;
                }
                "listening" => {
                    if !seen_wake {
                        failures.push(format!(
                            "session {}: Listening received before WakeWordDetected",
                            session
                        ));
                    }
                    seen_listen = true;
                }
                "idle" => {
                    if seen_listen && !seen_wake {
                        failures.push(format!(
                            "session {}: Idle without prior WakeWordDetected",
                            session
                        ));
                    }
                    seen_wake = false;
                    seen_listen = false;
                }
                _ => {}
            }
        }

        AssertionResult::with_failures(ID, DESC, failures)
    }
}
