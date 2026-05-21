//! OCR runtime — offline text extraction from screen captures.
//!
//! Architecture: `OcrBackend` trait → `StubOcrBackend` (test) / future
//! `TesseractBackend` (system Tesseract install).
//!
//! PRIVACY: OCR runs entirely on-device; no data is sent to cloud APIs.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::OnceCell;

use crate::screen_capture::CaptureResult;

pub static OCR_RUNS:             AtomicU64 = AtomicU64::new(0);
pub static OCR_SUCCESSES:        AtomicU64 = AtomicU64::new(0);
pub static OCR_FAILURES:         AtomicU64 = AtomicU64::new(0);
pub static OCR_TOTAL_LATENCY_MS: AtomicU64 = AtomicU64::new(0);

// ── OCR word ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrWord {
    pub text:       String,
    pub confidence: f32,
    /// Normalised bounding box [0.0–1.0] relative to image dimensions.
    pub x: f32, pub y: f32, pub w: f32, pub h: f32,
}

// ── OCR result ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrResult {
    pub text:       String,
    pub confidence: f32,
    pub words:      Vec<OcrWord>,
    pub latency_ms: u64,
    pub backend:    String,
    pub success:    bool,
    pub error:      Option<String>,
}

impl OcrResult {
    pub fn failed(backend: impl Into<String>, reason: impl Into<String>) -> Self {
        Self { text: String::new(), confidence: 0.0, words: vec![],
               latency_ms: 0, backend: backend.into(), success: false,
               error: Some(reason.into()) }
    }

    pub fn is_confident(&self) -> bool {
        self.success && self.confidence >= 0.60
    }

    pub fn contains_text(&self, query: &str) -> bool {
        self.text.to_lowercase().contains(&query.to_lowercase())
    }

    pub fn any_word_matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.words.iter().any(|w| w.text.to_lowercase().contains(&q))
    }
}

// ── Backend trait ─────────────────────────────────────────────────────────────

pub trait OcrBackend: Send + Sync {
    fn run(&self, capture: &CaptureResult) -> OcrResult;
    fn backend_name(&self) -> &'static str;
    fn supports_multilingual(&self) -> bool;
}

// ── Stub backend ──────────────────────────────────────────────────────────────

/// Returns fixed text injected at construction time.  Used in tests.
pub struct StubOcrBackend {
    pub stub_text:       String,
    pub stub_confidence: f32,
}

impl StubOcrBackend {
    pub fn new(text: impl Into<String>) -> Self {
        Self { stub_text: text.into(), stub_confidence: 0.92 }
    }

    pub fn empty() -> Self {
        Self { stub_text: String::new(), stub_confidence: 0.0 }
    }
}

impl OcrBackend for StubOcrBackend {
    fn run(&self, _capture: &CaptureResult) -> OcrResult {
        let words: Vec<OcrWord> = self.stub_text
            .split_whitespace()
            .map(|w| OcrWord { text: w.to_string(), confidence: self.stub_confidence,
                               x: 0.0, y: 0.0, w: 0.1, h: 0.05 })
            .collect();

        OcrResult {
            text: self.stub_text.clone(),
            confidence: self.stub_confidence,
            words,
            latency_ms: 1,
            backend: "stub".to_string(),
            success: !self.stub_text.is_empty(),
            error: if self.stub_text.is_empty() { Some("empty capture".to_string()) } else { None },
        }
    }

    fn backend_name(&self) -> &'static str { "stub" }
    fn supports_multilingual(&self) -> bool { true }
}

// ── Tesseract backend stub ────────────────────────────────────────────────────

/// Tesseract-based backend (requires `tesseract` CLI or leptonica bindings).
/// Not initialised by default — only available when Tesseract is installed.
pub struct TesseractBackend;

impl OcrBackend for TesseractBackend {
    fn run(&self, _capture: &CaptureResult) -> OcrResult {
        // Real implementation would call tesseract C API via tesseract-rs crate.
        // Returns failed result until the native library is linked.
        OcrResult::failed("tesseract", "tesseract native library not linked")
    }
    fn backend_name(&self) -> &'static str { "tesseract" }
    fn supports_multilingual(&self) -> bool { true }
}

// ── Runtime ───────────────────────────────────────────────────────────────────

static BACKEND: OnceCell<Box<dyn OcrBackend>> = OnceCell::new();

pub fn init_stub(text: impl Into<String>) {
    BACKEND.get_or_init(|| Box::new(StubOcrBackend::new(text)));
}

pub fn init_with(backend: Box<dyn OcrBackend>) {
    BACKEND.get_or_init(|| backend);
}

fn backend() -> &'static dyn OcrBackend {
    BACKEND.get_or_init(|| Box::new(StubOcrBackend::empty())).as_ref()
}

pub fn run_ocr(capture: &CaptureResult) -> OcrResult {
    OCR_RUNS.fetch_add(1, Ordering::Relaxed);
    let t0 = std::time::Instant::now();
    let mut result = backend().run(capture);
    let ms = t0.elapsed().as_millis() as u64;
    result.latency_ms = ms;
    OCR_TOTAL_LATENCY_MS.fetch_add(ms, Ordering::Relaxed);
    if result.success {
        OCR_SUCCESSES.fetch_add(1, Ordering::Relaxed);
    } else {
        OCR_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    result
}

pub fn ocr_active_window() -> OcrResult {
    let capture = crate::screen_capture::capture_active_window();
    run_ocr(&capture)
}

pub fn ocr_full_screen() -> OcrResult {
    let capture = crate::screen_capture::capture_full_screen();
    run_ocr(&capture)
}

pub fn backend_name() -> &'static str {
    backend().backend_name()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::OcrBackend;
    use crate::screen_capture::{CaptureBackend, CaptureMode, CaptureResult};

    fn dummy_capture() -> CaptureResult {
        CaptureResult { data: vec![0u8; 4], width: 800, height: 600,
            mode: CaptureMode::ActiveWindow, latency_ms: 1, success: true, error: None }
    }

    #[test]
    fn stub_ocr_returns_injected_text() {
        let b = StubOcrBackend::new("Visual Studio Code — workspace loaded");
        let r = b.run(&dummy_capture());
        assert!(r.success);
        assert!(r.contains_text("Visual Studio Code"));
    }

    #[test]
    fn ocr_result_confidence_check() {
        let b = StubOcrBackend::new("hello world");
        let r = b.run(&dummy_capture());
        assert!(r.is_confident());
    }

    #[test]
    fn ocr_word_match_is_case_insensitive() {
        let b = StubOcrBackend::new("Error: file not found");
        let r = b.run(&dummy_capture());
        assert!(r.any_word_matches("error"));
        assert!(r.any_word_matches("ERROR"));
    }

    #[test]
    fn ocr_empty_stub_not_confident() {
        let b = StubOcrBackend::empty();
        let r = b.run(&dummy_capture());
        assert!(!r.is_confident());
    }

    #[test]
    fn ocr_runs_counter_increments() {
        let before = OCR_RUNS.load(Ordering::Relaxed);
        run_ocr(&dummy_capture());
        assert!(OCR_RUNS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn failed_result_has_error() {
        let r = OcrResult::failed("test", "no capture data");
        assert!(!r.success);
        assert!(r.error.is_some());
    }
}
