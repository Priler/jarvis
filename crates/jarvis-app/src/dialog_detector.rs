//! Dialog detector — classifies visible dialogs using OCR.
//!
//! Detects crash dialogs, permission requests, warnings, confirmations,
//! and loading failures by pattern-matching OCR text.
//!
//! This module never auto-dismisses dialogs — it only reports them.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::ocr_runtime::{self, OcrResult};
use crate::screen_capture;
use crate::ui_state::DialogKind;

pub static DIALOG_CHECKS:    AtomicU64 = AtomicU64::new(0);
pub static DIALOGS_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static DIALOGS_NONE:     AtomicU64 = AtomicU64::new(0);

// ── Detected dialog ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectedDialog {
    pub kind:       DialogKind,
    pub title_hint: String,
    pub evidence:   Vec<String>,
    pub confidence: f32,
    pub is_blocking: bool,
}

impl DetectedDialog {
    pub fn requires_user_action(&self) -> bool {
        self.is_blocking
            || matches!(self.kind, DialogKind::Permission | DialogKind::Crash
                                 | DialogKind::Confirmation)
    }
}

// ── Pattern sets ──────────────────────────────────────────────────────────────

const CRASH_PATTERNS: &[&str] = &[
    "has stopped working", "stopped responding", "crash", "unhandled exception",
    "fatal error", "application error", "memory access violation",
];

const PERMISSION_PATTERNS: &[&str] = &[
    "allow", "administrator", "uac", "user account control",
    "do you want to allow", "run as administrator", "permission",
];

const WARNING_PATTERNS: &[&str] = &[
    "warning", "caution", "deprecated", "unsupported",
    "are you sure", "this action cannot",
];

const CONFIRMATION_PATTERNS: &[&str] = &[
    "save changes", "do you want to save", "confirm", "yes / no",
    "yes  no", "cancel", "discard",
];

const LOADING_FAILURE_PATTERNS: &[&str] = &[
    "failed to load", "could not load", "unable to connect",
    "timeout", "not found", "file not found",
];

// ── Dialog detector ───────────────────────────────────────────────────────────

pub struct DialogDetector;

impl DialogDetector {
    /// Scan the active window for dialogs and return the most prominent one.
    pub fn scan() -> Option<DetectedDialog> {
        DIALOG_CHECKS.fetch_add(1, Ordering::Relaxed);

        let capture = screen_capture::capture_active_window();
        if !capture.success {
            return None;
        }

        let ocr = ocr_runtime::run_ocr(&capture);
        Self::classify(&ocr)
    }

    /// Classify an OCR result into a dialog type.
    pub fn classify(ocr: &OcrResult) -> Option<DetectedDialog> {
        if !ocr.success || ocr.text.is_empty() {
            return None;
        }

        let text_lower = ocr.text.to_lowercase();

        // Priority order: Crash > Permission > Confirmation > Warning > LoadingFailure
        let candidates: &[(&[&str], DialogKind, bool)] = &[
            (CRASH_PATTERNS,           DialogKind::Crash,         true),
            (PERMISSION_PATTERNS,      DialogKind::Permission,    true),
            (CONFIRMATION_PATTERNS,    DialogKind::Confirmation,  true),
            (WARNING_PATTERNS,         DialogKind::Warning,       false),
            (LOADING_FAILURE_PATTERNS, DialogKind::Error,         false),
        ];

        for (patterns, kind, blocking) in candidates {
            let matched: Vec<String> = patterns.iter()
                .filter(|&&p| text_lower.contains(p))
                .map(|&p| p.to_string())
                .collect();

            if !matched.is_empty() {
                DIALOGS_DETECTED.fetch_add(1, Ordering::Relaxed);
                return Some(DetectedDialog {
                    kind: kind.clone(),
                    title_hint: extract_title(&ocr.text),
                    evidence: matched,
                    confidence: ocr.confidence,
                    is_blocking: *blocking,
                });
            }
        }

        DIALOGS_NONE.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Detect dialogs in arbitrary OCR text (for testing without screen capture).
    pub fn classify_text(text: &str, confidence: f32) -> Option<DetectedDialog> {
        let stub_ocr = OcrResult {
            text: text.to_string(),
            confidence,
            words: vec![],
            latency_ms: 0,
            backend: "direct".to_string(),
            success: true,
            error: None,
        };
        Self::classify(&stub_ocr)
    }
}

fn extract_title(text: &str) -> String {
    text.lines()
        .next()
        .map(|l| l.chars().take(50).collect())
        .unwrap_or_default()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_dialog_detected_from_text() {
        let d = DialogDetector::classify_text("Application has stopped working", 0.9);
        assert!(d.is_some());
        assert_eq!(d.unwrap().kind, DialogKind::Crash);
    }

    #[test]
    fn permission_dialog_detected() {
        let d = DialogDetector::classify_text(
            "Do you want to allow this app to make changes? Run as administrator", 0.85,
        );
        assert!(d.is_some());
        let d = d.unwrap();
        assert_eq!(d.kind, DialogKind::Permission);
        assert!(d.is_blocking);
        assert!(d.requires_user_action());
    }

    #[test]
    fn confirmation_dialog_detected() {
        let d = DialogDetector::classify_text("Save changes to file? Yes  No  Cancel", 0.88);
        assert!(d.is_some());
        assert!(matches!(d.unwrap().kind, DialogKind::Confirmation));
    }

    #[test]
    fn clean_screen_no_dialog() {
        let d = DialogDetector::classify_text("Welcome to Visual Studio Code", 0.95);
        assert!(d.is_none());
    }

    #[test]
    fn loading_failure_detected() {
        let d = DialogDetector::classify_text("Failed to load project: file not found", 0.9);
        assert!(d.is_some());
        assert_eq!(d.unwrap().kind, DialogKind::Error);
    }

    #[test]
    fn dialog_checks_counter_increments() {
        use crate::screen_capture;
        screen_capture::init_stub();
        use crate::ocr_runtime;
        ocr_runtime::init_stub("no dialog here");
        let before = DIALOG_CHECKS.load(Ordering::Relaxed);
        DialogDetector::scan();
        assert!(DIALOG_CHECKS.load(Ordering::Relaxed) > before);
    }
}
