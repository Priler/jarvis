//! Screen capture engine — offline desktop snapshots.
//!
//! Abstracts OS-specific capture behind a `CaptureBackend` trait.
//! Ships with a `StubCaptureBackend` for testing; real backends
//! (Win32 BitBlt, X11 XGetImage) are plugged in via `init_with`.
//!
//! PRIVACY: captured pixel data never leaves the local machine.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::OnceCell;

pub static CAPTURES_REQUESTED:       AtomicU64 = AtomicU64::new(0);
pub static CAPTURES_SUCCEEDED:       AtomicU64 = AtomicU64::new(0);
pub static CAPTURES_FAILED:          AtomicU64 = AtomicU64::new(0);
pub static CAPTURE_TOTAL_LATENCY_MS: AtomicU64 = AtomicU64::new(0);

// ── Region ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenRegion {
    pub x:      i32,
    pub y:      i32,
    pub width:  u32,
    pub height: u32,
}

impl ScreenRegion {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
    pub fn full_screen() -> Self {
        Self { x: 0, y: 0, width: 1920, height: 1080 }
    }
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

// ── Mode ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum CaptureMode {
    FullScreen,
    ActiveWindow,
    Region,
}

// ── Result ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct CaptureResult {
    /// Raw pixel bytes (BGRA). Empty in stub mode.
    pub data:       Vec<u8>,
    pub width:      u32,
    pub height:     u32,
    pub mode:       CaptureMode,
    pub latency_ms: u64,
    pub success:    bool,
    pub error:      Option<String>,
}

impl CaptureResult {
    pub fn failed(mode: CaptureMode, reason: impl Into<String>) -> Self {
        Self { data: vec![], width: 0, height: 0, mode, latency_ms: 0,
               success: false, error: Some(reason.into()) }
    }
    pub fn has_data(&self) -> bool { !self.data.is_empty() }
    pub fn pixel_count(&self) -> usize { self.data.len() / 4 }
}

// ── Backend trait ─────────────────────────────────────────────────────────────

pub trait CaptureBackend: Send + Sync {
    fn capture_full_screen(&self)              -> CaptureResult;
    fn capture_active_window(&self)            -> CaptureResult;
    fn capture_region(&self, r: &ScreenRegion) -> CaptureResult;
    fn backend_name(&self)                     -> &'static str;
}

// ── Stub backend ──────────────────────────────────────────────────────────────

pub struct StubCaptureBackend;

impl CaptureBackend for StubCaptureBackend {
    fn capture_full_screen(&self) -> CaptureResult {
        CaptureResult { data: vec![0u8; 4], width: 1920, height: 1080,
            mode: CaptureMode::FullScreen, latency_ms: 1, success: true, error: None }
    }
    fn capture_active_window(&self) -> CaptureResult {
        CaptureResult { data: vec![0u8; 4], width: 800, height: 600,
            mode: CaptureMode::ActiveWindow, latency_ms: 1, success: true, error: None }
    }
    fn capture_region(&self, r: &ScreenRegion) -> CaptureResult {
        CaptureResult { data: vec![0u8; 4], width: r.width, height: r.height,
            mode: CaptureMode::Region, latency_ms: 1, success: true, error: None }
    }
    fn backend_name(&self) -> &'static str { "stub" }
}

// ── Engine ────────────────────────────────────────────────────────────────────

static BACKEND: OnceCell<Box<dyn CaptureBackend>> = OnceCell::new();

pub fn init_stub() {
    BACKEND.get_or_init(|| Box::new(StubCaptureBackend));
}

pub fn init_with(backend: Box<dyn CaptureBackend>) {
    BACKEND.get_or_init(|| backend);
}

fn backend() -> &'static dyn CaptureBackend {
    BACKEND.get_or_init(|| Box::new(StubCaptureBackend)).as_ref()
}

fn timed<F: FnOnce() -> CaptureResult>(f: F) -> CaptureResult {
    let t0 = std::time::Instant::now();
    let mut r = f();
    let ms = t0.elapsed().as_millis() as u64;
    r.latency_ms = ms;
    CAPTURE_TOTAL_LATENCY_MS.fetch_add(ms, Ordering::Relaxed);
    r
}

pub fn capture_full_screen() -> CaptureResult {
    CAPTURES_REQUESTED.fetch_add(1, Ordering::Relaxed);
    let r = timed(|| backend().capture_full_screen());
    if r.success { CAPTURES_SUCCEEDED.fetch_add(1, Ordering::Relaxed); }
    else         { CAPTURES_FAILED.fetch_add(1, Ordering::Relaxed); }
    r
}

pub fn capture_active_window() -> CaptureResult {
    CAPTURES_REQUESTED.fetch_add(1, Ordering::Relaxed);
    let r = timed(|| backend().capture_active_window());
    if r.success { CAPTURES_SUCCEEDED.fetch_add(1, Ordering::Relaxed); }
    else         { CAPTURES_FAILED.fetch_add(1, Ordering::Relaxed); }
    r
}

pub fn capture_region(region: &ScreenRegion) -> CaptureResult {
    CAPTURES_REQUESTED.fetch_add(1, Ordering::Relaxed);
    let r = timed(|| backend().capture_region(region));
    if r.success { CAPTURES_SUCCEEDED.fetch_add(1, Ordering::Relaxed); }
    else         { CAPTURES_FAILED.fetch_add(1, Ordering::Relaxed); }
    r
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_full_screen_succeeds() {
        let r = StubCaptureBackend.capture_full_screen();
        assert!(r.success);
        assert_eq!(r.mode, CaptureMode::FullScreen);
        assert_eq!(r.width, 1920);
    }

    #[test]
    fn stub_active_window_has_dimensions() {
        let r = StubCaptureBackend.capture_active_window();
        assert!(r.width > 0 && r.height > 0);
        assert!(r.success);
    }

    #[test]
    fn stub_region_respects_dimensions() {
        let region = ScreenRegion::new(0, 0, 320, 240);
        let r = StubCaptureBackend.capture_region(&region);
        assert_eq!(r.width, 320);
        assert_eq!(r.height, 240);
    }

    #[test]
    fn failed_result_has_error_message() {
        let r = CaptureResult::failed(CaptureMode::FullScreen, "permission denied");
        assert!(!r.success);
        assert!(r.error.as_deref().unwrap().contains("permission"));
    }

    #[test]
    fn captures_requested_counter_increments() {
        init_stub();
        let before = CAPTURES_REQUESTED.load(Ordering::Relaxed);
        capture_full_screen();
        assert!(CAPTURES_REQUESTED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn screen_region_area_is_correct() {
        let r = ScreenRegion::new(0, 0, 100, 200);
        assert_eq!(r.area(), 20_000);
    }
}
