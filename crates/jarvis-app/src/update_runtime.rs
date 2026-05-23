//! Auto-update runtime — version tracking, update check scheduling,
//! delta download simulation, and rollback version management.
//! All operations are local; no network connections are made.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static UPDATE_CHECKS_RUN:   AtomicU64  = AtomicU64::new(0);
pub static UPDATES_APPLIED:     AtomicU64  = AtomicU64::new(0);
pub static ROLLBACKS_PERFORMED: AtomicU64  = AtomicU64::new(0);
pub static AUTO_UPDATE_ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum UpdateChannel { Stable, Beta, Nightly }

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum UpdateStatus {
    UpToDate,
    Available { version: String, size_kb: u64 },
    Downloading { version: String, progress_pct: u8 },
    ReadyToInstall { version: String },
    Failed { reason: String },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VersionRecord {
    pub version:     String,
    pub build_date:  String,
    pub channel:     UpdateChannel,
    pub applied_at:  u64,
    pub is_current:  bool,
}

const VERSION_HISTORY_MAX: usize = 10;

struct UpdateState {
    current_version:  String,
    channel:          UpdateChannel,
    status:           UpdateStatus,
    version_history:  Vec<VersionRecord>,
    last_check_ts:    u64,
    check_interval_s: u64,
}

impl UpdateState {
    fn new() -> Self {
        let mut history = Vec::new();
        history.push(VersionRecord {
            version:    "1.0.0".to_string(),
            build_date: "2026-05-22".to_string(),
            channel:    UpdateChannel::Stable,
            applied_at: ts_now(),
            is_current: true,
        });
        Self {
            current_version:  "1.0.0".to_string(),
            channel:          UpdateChannel::Stable,
            status:           UpdateStatus::UpToDate,
            version_history:  history,
            last_check_ts:    0,
            check_interval_s: 86_400, // 24 h
        }
    }
}

static STATE: Lazy<Mutex<UpdateState>> = Lazy::new(|| Mutex::new(UpdateState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Version query ─────────────────────────────────────────────────────────────

pub fn current_version() -> String { STATE.lock().unwrap().current_version.clone() }

pub fn channel() -> UpdateChannel { STATE.lock().unwrap().channel.clone() }

pub fn set_channel(ch: UpdateChannel) {
    STATE.lock().unwrap().channel = ch;
}

pub fn set_auto_update(enabled: bool) {
    AUTO_UPDATE_ENABLED.store(enabled, Ordering::Relaxed);
}

// ── Check logic ───────────────────────────────────────────────────────────────

pub fn should_check() -> bool {
    let s = STATE.lock().unwrap();
    let elapsed = ts_now().saturating_sub(s.last_check_ts);
    elapsed >= s.check_interval_s
}

pub fn run_check() -> UpdateStatus {
    let mut s = STATE.lock().unwrap();
    s.last_check_ts = ts_now();
    UPDATE_CHECKS_RUN.fetch_add(1, Ordering::Relaxed);
    // Offline: never reports an available update unless one was staged.
    // status stays UpToDate unless manually staged.
    crate::production_logging::info("update_runtime",
        &format!("update check complete — {}", &s.current_version));
    s.status.clone()
}

// ── Stage / apply (test/staging use) ──────────────────────────────────────────

pub fn stage_update(version: &str, size_kb: u64) {
    let mut s = STATE.lock().unwrap();
    s.status = UpdateStatus::Available {
        version: version.to_string(),
        size_kb,
    };
}

pub fn begin_download(version: &str) {
    let mut s = STATE.lock().unwrap();
    s.status = UpdateStatus::Downloading { version: version.to_string(), progress_pct: 0 };
}

pub fn set_download_progress(pct: u8) {
    let mut s = STATE.lock().unwrap();
    if let UpdateStatus::Downloading { ref version, .. } = s.status.clone() {
        let v = version.clone();
        if pct >= 100 {
            s.status = UpdateStatus::ReadyToInstall { version: v };
        } else {
            s.status = UpdateStatus::Downloading { version: v, progress_pct: pct };
        }
    }
}

pub fn apply_update() -> bool {
    let mut s = STATE.lock().unwrap();
    if let UpdateStatus::ReadyToInstall { ref version } = s.status.clone() {
        let new_ver = version.clone();
        let channel = s.channel.clone();
        // Mark old version as not current
        for rec in s.version_history.iter_mut() { rec.is_current = false; }
        // Add new version record
        if s.version_history.len() >= VERSION_HISTORY_MAX { s.version_history.remove(0); }
        s.version_history.push(VersionRecord {
            version:    new_ver.clone(),
            build_date: "2026-05-22".to_string(),
            channel,
            applied_at: ts_now(),
            is_current: true,
        });
        s.current_version = new_ver;
        s.status = UpdateStatus::UpToDate;
        UPDATES_APPLIED.fetch_add(1, Ordering::Relaxed);
        crate::production_logging::info("update_runtime",
            &format!("update applied: {}", &s.current_version));
        true
    } else {
        false
    }
}

// ── Rollback ──────────────────────────────────────────────────────────────────

pub fn rollback() -> Option<String> {
    let mut s = STATE.lock().unwrap();
    // Find previous version (second-most-recent)
    let len = s.version_history.len();
    if len < 2 { return None; }
    let prev_version = s.version_history[len - 2].version.clone();
    for rec in s.version_history.iter_mut() { rec.is_current = false; }
    if let Some(rec) = s.version_history.iter_mut().find(|r| r.version == prev_version) {
        rec.is_current = true;
    }
    s.current_version = prev_version.clone();
    s.status = UpdateStatus::UpToDate;
    ROLLBACKS_PERFORMED.fetch_add(1, Ordering::Relaxed);
    crate::production_logging::info("update_runtime",
        &format!("rolled back to {}", prev_version));
    Some(prev_version)
}

pub fn version_history() -> Vec<VersionRecord> {
    STATE.lock().unwrap().version_history.clone()
}

pub fn update_status() -> UpdateStatus {
    STATE.lock().unwrap().status.clone()
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct UpdateSnapshot {
    pub current_version:    String,
    pub channel:            UpdateChannel,
    pub auto_update:        bool,
    pub update_checks_run:  u64,
    pub updates_applied:    u64,
    pub rollbacks_performed: u64,
    pub status:             UpdateStatus,
}

pub fn snapshot() -> UpdateSnapshot {
    let s = STATE.lock().unwrap();
    UpdateSnapshot {
        current_version:     s.current_version.clone(),
        channel:             s.channel.clone(),
        auto_update:         AUTO_UPDATE_ENABLED.load(Ordering::Relaxed),
        update_checks_run:   UPDATE_CHECKS_RUN.load(Ordering::Relaxed),
        updates_applied:     UPDATES_APPLIED.load(Ordering::Relaxed),
        rollbacks_performed: ROLLBACKS_PERFORMED.load(Ordering::Relaxed),
        status:              s.status.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_is_semver() {
        let v = current_version();
        assert!(v.contains('.'), "expected semver, got {v}");
    }

    #[test]
    fn stage_and_apply_update() {
        stage_update("1.0.1", 2048);
        if let UpdateStatus::Available { ref version, .. } = update_status() {
            assert_eq!(version, "1.0.1");
        } else {
            panic!("expected Available status");
        }
        begin_download("1.0.1");
        set_download_progress(100);
        assert!(apply_update());
        assert_eq!(current_version(), "1.0.1");
        assert_eq!(update_status(), UpdateStatus::UpToDate);
    }

    #[test]
    fn rollback_to_previous() {
        // ensure at least two versions exist from prior test or add them
        stage_update("1.0.2", 512);
        begin_download("1.0.2");
        set_download_progress(100);
        apply_update();
        let rolled = rollback();
        assert!(rolled.is_some());
        assert_ne!(current_version(), "1.0.2");
    }

    #[test]
    fn run_check_increments_counter() {
        let before = UPDATE_CHECKS_RUN.load(Ordering::Relaxed);
        run_check();
        assert!(UPDATE_CHECKS_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn auto_update_toggle() {
        set_auto_update(false);
        assert!(!AUTO_UPDATE_ENABLED.load(Ordering::Relaxed));
        set_auto_update(true);
        assert!(AUTO_UPDATE_ENABLED.load(Ordering::Relaxed));
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(!s.current_version.is_empty());
    }
}
