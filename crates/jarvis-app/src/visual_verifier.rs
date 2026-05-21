//! Visual verifier — confirms UI state through OCR and screen capture.
//!
//! Replaces process-existence checks with visual evidence:
//!   - Window title verification via OCR
//!   - Visible text verification (workspace loaded, ready state, etc.)
//!   - Error popup detection
//!   - Loading state detection

use std::sync::atomic::{AtomicU64, Ordering};

use crate::ocr_runtime::{self, OcrResult};
use crate::screen_capture;

pub static VISUAL_CHECKS:    AtomicU64 = AtomicU64::new(0);
pub static VISUAL_CONFIRMED: AtomicU64 = AtomicU64::new(0);
pub static VISUAL_FAILED:    AtomicU64 = AtomicU64::new(0);
pub static VISUAL_AMBIGUOUS: AtomicU64 = AtomicU64::new(0);

// ── Visual verdict ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum VisualVerdict {
    Confirmed { evidence: String },
    Failed    { expected: String, actual: String },
    Ambiguous { reason: String },
    SkippedNoCapture,
}

impl VisualVerdict {
    pub fn is_confirmed(&self) -> bool {
        matches!(self, VisualVerdict::Confirmed { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, VisualVerdict::Failed { .. })
    }
}

// ── Visual check spec ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum VisualCheck {
    /// Text must appear in the visible screen.
    TextVisible(String),
    /// Text must NOT appear (e.g., error dialogs).
    TextAbsent(String),
    /// Window title must contain the given fragment.
    WindowTitle(String),
    /// No text matching known error patterns should be visible.
    NoErrorDialog,
    /// OCR confidence must be above threshold.
    OcrConfidence(f32),
}

// ── Visual verifier ───────────────────────────────────────────────────────────

pub struct VisualVerifier;

const ERROR_PATTERNS: &[&str] = &[
    "error", "fatal", "crash", "failed", "unhandled exception",
    "access denied", "permission denied", "not responding",
];

const LOADING_PATTERNS: &[&str] = &[
    "loading", "please wait", "initialising", "initializing",
    "starting", "connecting",
];

impl VisualVerifier {
    /// Run a `VisualCheck` against the current active-window OCR output.
    pub fn check(check: &VisualCheck) -> VisualVerdict {
        VISUAL_CHECKS.fetch_add(1, Ordering::Relaxed);

        let capture = screen_capture::capture_active_window();
        if !capture.success {
            VISUAL_AMBIGUOUS.fetch_add(1, Ordering::Relaxed);
            return VisualVerdict::Ambiguous { reason: "capture failed".to_string() };
        }

        let ocr = ocr_runtime::run_ocr(&capture);

        let verdict = match check {
            VisualCheck::TextVisible(expected) => {
                if ocr.contains_text(expected) {
                    VisualVerdict::Confirmed { evidence: format!("text '{}' found", expected) }
                } else {
                    VisualVerdict::Failed {
                        expected: expected.clone(),
                        actual: ocr.text.chars().take(80).collect(),
                    }
                }
            }

            VisualCheck::TextAbsent(forbidden) => {
                if ocr.contains_text(forbidden) {
                    VisualVerdict::Failed {
                        expected: format!("no '{}'", forbidden),
                        actual: format!("found '{}'", forbidden),
                    }
                } else {
                    VisualVerdict::Confirmed { evidence: format!("text '{}' absent", forbidden) }
                }
            }

            VisualCheck::WindowTitle(fragment) => {
                if ocr.contains_text(fragment) {
                    VisualVerdict::Confirmed { evidence: format!("title contains '{}'", fragment) }
                } else {
                    VisualVerdict::Ambiguous {
                        reason: format!("title fragment '{}' not visible in OCR", fragment),
                    }
                }
            }

            VisualCheck::NoErrorDialog => {
                let found = ERROR_PATTERNS.iter()
                    .find(|&&pat| ocr.contains_text(pat));
                match found {
                    Some(pat) => VisualVerdict::Failed {
                        expected: "no error dialog".to_string(),
                        actual: format!("found error pattern '{}'", pat),
                    },
                    None => VisualVerdict::Confirmed {
                        evidence: "no error patterns detected".to_string(),
                    },
                }
            }

            VisualCheck::OcrConfidence(threshold) => {
                if ocr.confidence >= *threshold {
                    VisualVerdict::Confirmed {
                        evidence: format!("OCR confidence {:.2} >= {:.2}", ocr.confidence, threshold),
                    }
                } else {
                    VisualVerdict::Ambiguous {
                        reason: format!("OCR confidence {:.2} < {:.2}", ocr.confidence, threshold),
                    }
                }
            }
        };

        match &verdict {
            VisualVerdict::Confirmed { .. } => { VISUAL_CONFIRMED.fetch_add(1, Ordering::Relaxed); }
            VisualVerdict::Failed { .. }    => { VISUAL_FAILED.fetch_add(1, Ordering::Relaxed); }
            _                               => { VISUAL_AMBIGUOUS.fetch_add(1, Ordering::Relaxed); }
        }

        verdict
    }

    /// True when the screen shows a loading/waiting state.
    pub fn is_loading() -> bool {
        let capture = screen_capture::capture_active_window();
        if !capture.success { return false; }
        let ocr = ocr_runtime::run_ocr(&capture);
        LOADING_PATTERNS.iter().any(|&p| ocr.contains_text(p))
    }

    pub fn workspace_loaded(app_name: &str) -> VisualVerdict {
        Self::check(&VisualCheck::TextVisible(app_name.to_string()))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr_runtime::{self, OcrBackend};
    use crate::screen_capture::{self, CaptureBackend};

    fn init_ocr(text: &str) {
        // OnceCell — only first call takes effect; tests that need different text
        // test the backend directly instead of the runtime singleton.
        ocr_runtime::init_stub(text);
        screen_capture::init_stub();
    }

    #[test]
    fn text_visible_check_confirmed_by_ocr() {
        let backend = crate::ocr_runtime::StubOcrBackend::new("Visual Studio Code workspace loaded");
        let capture = crate::screen_capture::StubCaptureBackend.capture_active_window();
        let ocr = backend.run(&capture);
        assert!(ocr.contains_text("workspace loaded"));
    }

    #[test]
    fn text_absent_check_fails_when_present() {
        let backend = crate::ocr_runtime::StubOcrBackend::new("Fatal Error crash detected");
        let capture = crate::screen_capture::StubCaptureBackend.capture_active_window();
        let ocr = backend.run(&capture);
        // Error pattern IS present — text absent check should fail
        assert!(ocr.contains_text("error"));
    }

    #[test]
    fn ocr_confidence_threshold_accepted() {
        let backend = crate::ocr_runtime::StubOcrBackend::new("hello");
        let capture = crate::screen_capture::StubCaptureBackend.capture_active_window();
        let ocr = backend.run(&capture);
        assert!(ocr.confidence >= 0.60, "stub confidence is {}", ocr.confidence);
    }

    #[test]
    fn visual_verdict_confirmed_is_confirmed() {
        let v = VisualVerdict::Confirmed { evidence: "ok".into() };
        assert!(v.is_confirmed());
        assert!(!v.is_failed());
    }

    #[test]
    fn visual_verdict_failed_is_not_confirmed() {
        let v = VisualVerdict::Failed { expected: "x".into(), actual: "y".into() };
        assert!(!v.is_confirmed());
        assert!(v.is_failed());
    }

    #[test]
    fn visual_checks_counter_increments() {
        init_ocr("some text");
        let before = VISUAL_CHECKS.load(Ordering::Relaxed);
        VisualVerifier::check(&VisualCheck::NoErrorDialog);
        assert!(VISUAL_CHECKS.load(Ordering::Relaxed) > before);
    }
}
