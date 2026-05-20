#![allow(dead_code)]

//! Failure classification for replay assertion results.
//!
//! Maps assertion IDs and violation descriptions to a high-level
//! failure class used in certification reports.

/// High-level failure category for a failed replay assertion or scenario violation.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureClass {
    /// Wake-word detection failed or produced unexpected results.
    Wake,
    /// Speech-to-text produced wrong, missing, or contaminated transcript.
    Stt,
    /// Command dispatch failed, produced a duplicate, or opened outside wake boundary.
    Command,
    /// A latency threshold was exceeded.
    Timing,
    /// An illegal state transition or session lifecycle violation occurred.
    State,
    /// An assertion that does not fit other categories failed.
    Assertion,
    /// The subprocess failed to complete or the replay framework itself errored.
    Replay,
    /// The same WAV produced different results across iterations (nondeterminism).
    Nondeterminism,
    /// The pipeline self-activated (wake / STT / command) on non-wake audio.
    SelfHearing,
}

impl FailureClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wake => "WAKE",
            Self::Stt => "STT",
            Self::Command => "COMMAND",
            Self::Timing => "TIMING",
            Self::State => "STATE",
            Self::Assertion => "ASSERTION",
            Self::Replay => "REPLAY",
            Self::Nondeterminism => "NONDETERMINISM",
            Self::SelfHearing => "SELF_HEARING",
        }
    }
}

impl std::fmt::Display for FailureClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Map an assertion ID to its failure class.
pub fn classify_assertion(id: &str) -> FailureClass {
    match id {
        // Wake-word assertion failures
        "A001" | "A009" | "A013" => FailureClass::Wake,
        // Recognizer / STT contamination
        "A007" | "A012" => FailureClass::Stt,
        // Command session integrity
        "A004" | "A011" | "A014" => FailureClass::Command,
        // Timing / debounce / gate
        "A006" | "A008" => FailureClass::Timing,
        // State machine lifecycle
        "A002" | "A003" | "A005" => FailureClass::State,
        // IPC / ordering
        "A010" => FailureClass::Assertion,
        // Subprocess infrastructure
        "SUBPROCESS" => FailureClass::Replay,
        _ => FailureClass::Assertion,
    }
}

/// Map a scenario violation message to a failure class.
/// Used for violation strings returned by `Scenario::validate()`.
pub fn classify_violation(msg: &str) -> FailureClass {
    if msg.contains("wake_count") || msg.contains("wake_sessions") {
        FailureClass::Wake
    } else if msg.contains("transcript") || msg.contains("stt") {
        FailureClass::Stt
    } else if msg.contains("command_count") || msg.contains("duplicate") || msg.contains("self_hearing") {
        FailureClass::Command
    } else if msg.contains("latency") || msg.contains("ms") {
        FailureClass::Timing
    } else if msg.contains("transition") || msg.contains("session") || msg.contains("lifecycle") {
        FailureClass::State
    } else {
        FailureClass::Assertion
    }
}
