//! Windows audio session management — device enumeration, per-app volume,
//! VAD (voice activity detection) integration, and audio pipeline state.
//! All operations are local; no external services contacted.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static AUDIO_SESSIONS_TRACKED: AtomicU64  = AtomicU64::new(0);
pub static VAD_EVENTS_TOTAL:        AtomicU64  = AtomicU64::new(0);
pub static DEVICE_CHANGES_SEEN:     AtomicU64  = AtomicU64::new(0);
pub static CAPTURE_ACTIVE:          AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDevice {
    pub id:        String,
    pub name:      String,
    pub is_default: bool,
    pub channels:  u8,
    pub sample_rate: u32,
    pub kind:      AudioDeviceKind,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum AudioDeviceKind { Capture, Render }

#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioSession {
    pub pid:        u32,
    pub name:       String,
    pub volume:     f32,
    pub muted:      bool,
    pub is_jarvis:  bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VadEvent {
    pub id:         u64,
    pub timestamp:  u64,
    pub kind:       VadEventKind,
    pub confidence: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum VadEventKind { SpeechStart, SpeechEnd, Wakeword, NoiseFloor }

const VAD_EVENT_BUFFER: usize = 64;

struct AudioState {
    devices:     Vec<AudioDevice>,
    sessions:    Vec<AudioSession>,
    vad_events:  Vec<VadEvent>,
    next_vad_id: u64,
    noise_floor: f32,
    capture_device_id: String,
}

impl AudioState {
    fn new() -> Self {
        Self {
            devices: vec![
                AudioDevice {
                    id: "default-capture".to_string(),
                    name: "Default Microphone".to_string(),
                    is_default: true,
                    channels: 1,
                    sample_rate: 16_000,
                    kind: AudioDeviceKind::Capture,
                },
                AudioDevice {
                    id: "default-render".to_string(),
                    name: "Default Speakers".to_string(),
                    is_default: true,
                    channels: 2,
                    sample_rate: 48_000,
                    kind: AudioDeviceKind::Render,
                },
            ],
            sessions: vec![
                AudioSession { pid: 0, name: "Jarvis".to_string(), volume: 1.0, muted: false, is_jarvis: true },
            ],
            vad_events:  Vec::new(),
            next_vad_id: 1,
            noise_floor: 0.02,
            capture_device_id: "default-capture".to_string(),
        }
    }
}

static STATE: Lazy<Mutex<AudioState>> = Lazy::new(|| Mutex::new(AudioState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Device management ─────────────────────────────────────────────────────────

pub fn list_capture_devices() -> Vec<AudioDevice> {
    STATE.lock().unwrap().devices.iter()
        .filter(|d| d.kind == AudioDeviceKind::Capture)
        .cloned()
        .collect()
}

pub fn list_render_devices() -> Vec<AudioDevice> {
    STATE.lock().unwrap().devices.iter()
        .filter(|d| d.kind == AudioDeviceKind::Render)
        .cloned()
        .collect()
}

pub fn set_capture_device(device_id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    if s.devices.iter().any(|d| d.id == device_id && d.kind == AudioDeviceKind::Capture) {
        s.capture_device_id = device_id.to_string();
        DEVICE_CHANGES_SEEN.fetch_add(1, Ordering::Relaxed);
        crate::production_logging::info("windows_audio_manager",
            &format!("capture device set to {}", device_id));
        true
    } else {
        false
    }
}

pub fn active_capture_device() -> Option<AudioDevice> {
    let s = STATE.lock().unwrap();
    let id = s.capture_device_id.clone();
    s.devices.iter().find(|d| d.id == id).cloned()
}

// ── Session management ────────────────────────────────────────────────────────

pub fn register_session(pid: u32, name: &str) {
    let mut s = STATE.lock().unwrap();
    if !s.sessions.iter().any(|se| se.pid == pid) {
        s.sessions.push(AudioSession {
            pid,
            name: name.to_string(),
            volume: 1.0,
            muted: false,
            is_jarvis: name.to_lowercase().contains("jarvis"),
        });
        AUDIO_SESSIONS_TRACKED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn set_session_volume(pid: u32, volume: f32) -> bool {
    let mut s = STATE.lock().unwrap();
    let vol = volume.clamp(0.0, 1.0);
    if let Some(se) = s.sessions.iter_mut().find(|se| se.pid == pid) {
        se.volume = vol;
        true
    } else {
        false
    }
}

pub fn mute_session(pid: u32, muted: bool) -> bool {
    let mut s = STATE.lock().unwrap();
    if let Some(se) = s.sessions.iter_mut().find(|se| se.pid == pid) {
        se.muted = muted;
        true
    } else {
        false
    }
}

pub fn list_sessions() -> Vec<AudioSession> {
    STATE.lock().unwrap().sessions.clone()
}

// ── VAD integration ───────────────────────────────────────────────────────────

pub fn start_capture() {
    CAPTURE_ACTIVE.store(true, Ordering::Relaxed);
    crate::production_logging::info("windows_audio_manager", "audio capture started");
}

pub fn stop_capture() {
    CAPTURE_ACTIVE.store(false, Ordering::Relaxed);
}

pub fn is_capture_active() -> bool { CAPTURE_ACTIVE.load(Ordering::Relaxed) }

pub fn report_vad_event(kind: VadEventKind, confidence: f32) -> u64 {
    let mut s = STATE.lock().unwrap();
    let id = s.next_vad_id;
    s.next_vad_id += 1;
    if s.vad_events.len() >= VAD_EVENT_BUFFER { s.vad_events.remove(0); }
    s.vad_events.push(VadEvent { id, timestamp: ts_now(), kind, confidence });
    VAD_EVENTS_TOTAL.fetch_add(1, Ordering::Relaxed);
    id
}

pub fn recent_vad_events(n: usize) -> Vec<VadEvent> {
    let s = STATE.lock().unwrap();
    s.vad_events.iter().rev().take(n).cloned().collect()
}

pub fn set_noise_floor(level: f32) {
    STATE.lock().unwrap().noise_floor = level.clamp(0.0, 1.0);
}

pub fn noise_floor() -> f32 { STATE.lock().unwrap().noise_floor }

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct AudioSnapshot {
    pub capture_active:        bool,
    pub audio_sessions_tracked: u64,
    pub vad_events_total:      u64,
    pub device_changes_seen:   u64,
    pub noise_floor:           f32,
    pub capture_device:        Option<String>,
}

pub fn snapshot() -> AudioSnapshot {
    let s = STATE.lock().unwrap();
    AudioSnapshot {
        capture_active:         CAPTURE_ACTIVE.load(Ordering::Relaxed),
        audio_sessions_tracked: AUDIO_SESSIONS_TRACKED.load(Ordering::Relaxed),
        vad_events_total:       VAD_EVENTS_TOTAL.load(Ordering::Relaxed),
        device_changes_seen:    DEVICE_CHANGES_SEEN.load(Ordering::Relaxed),
        noise_floor:            s.noise_floor,
        capture_device:         Some(s.capture_device_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_capture_devices_nonempty() {
        let devices = list_capture_devices();
        assert!(!devices.is_empty());
    }

    #[test]
    fn set_capture_device_valid() {
        assert!(set_capture_device("default-capture"));
    }

    #[test]
    fn set_capture_device_invalid() {
        assert!(!set_capture_device("nonexistent-device-xyz"));
    }

    #[test]
    fn register_and_volume_session() {
        register_session(9999, "TestApp");
        assert!(set_session_volume(9999, 0.5));
        let sessions = list_sessions();
        let se = sessions.iter().find(|s| s.pid == 9999);
        assert!(se.is_some());
        assert!((se.unwrap().volume - 0.5).abs() < 0.001);
    }

    #[test]
    fn vad_event_recorded() {
        let before = VAD_EVENTS_TOTAL.load(Ordering::Relaxed);
        report_vad_event(VadEventKind::SpeechStart, 0.9);
        assert!(VAD_EVENTS_TOTAL.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn capture_active_toggle() {
        start_capture();
        assert!(is_capture_active());
        stop_capture();
        assert!(!is_capture_active());
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.noise_floor >= 0.0);
    }
}
