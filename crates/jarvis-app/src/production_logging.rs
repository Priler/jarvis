//! Production logging — structured log sink with memory ring buffer + file export.
//! Captures runtime crashes, model failures, voice failures, scheduler overload,
//! memory corruption, and permission violations.

use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use once_cell::sync::Lazy;

pub static LOG_ENTRIES_TOTAL:    AtomicU64 = AtomicU64::new(0);
pub static LOG_ERRORS_TOTAL:     AtomicU64 = AtomicU64::new(0);
pub static LOG_CRITICALS_TOTAL:  AtomicU64 = AtomicU64::new(0);

const RING_BUFFER_SIZE: usize = 1000;
const LOG_FILE: &str          = "jarvis_production.log";

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LogLevel { Info, Warning, Error, Critical }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub ts_ms:     u64,
    pub level:     LogLevel,
    pub component: String,
    pub message:   String,
    pub tags:      Vec<String>,
}

static BUFFER: Lazy<Mutex<Vec<LogEntry>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn log_file_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("jarvis").join(LOG_FILE)
}

// ── Internal write ────────────────────────────────────────────────────────────

fn append_to_file(entry: &LogEntry) {
    let path = log_file_path();
    if let Some(p) = path.parent() { let _ = std::fs::create_dir_all(p); }
    if let Ok(line) = serde_json::to_string(entry) {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "{}", line);
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn log(level: LogLevel, component: &str, message: &str, tags: Vec<String>) {
    let entry = LogEntry { ts_ms: ts_now(), level: level.clone(), component: component.to_string(), message: message.to_string(), tags };

    match &entry.level {
        LogLevel::Error    => { LOG_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed); }
        LogLevel::Critical => { LOG_CRITICALS_TOTAL.fetch_add(1, Ordering::Relaxed); LOG_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed); }
        _ => {}
    }
    LOG_ENTRIES_TOTAL.fetch_add(1, Ordering::Relaxed);

    // Write to file for Error and Critical
    if matches!(entry.level, LogLevel::Error | LogLevel::Critical) {
        append_to_file(&entry);
    }

    let mut buf = BUFFER.lock().unwrap();
    if buf.len() >= RING_BUFFER_SIZE { buf.remove(0); }
    buf.push(entry);
}

pub fn info(component: &str, msg: &str) {
    log(LogLevel::Info, component, msg, vec![]);
}

pub fn warn(component: &str, msg: &str) {
    log(LogLevel::Warning, component, msg, vec![]);
}

pub fn error(component: &str, msg: &str) {
    log(LogLevel::Error, component, msg, vec!["error".to_string()]);
}

pub fn critical(component: &str, msg: &str) {
    log(LogLevel::Critical, component, msg, vec!["critical".to_string()]);
}

pub fn recent(n: usize) -> Vec<LogEntry> {
    let buf = BUFFER.lock().unwrap();
    let start = buf.len().saturating_sub(n);
    buf[start..].to_vec()
}

pub fn recent_errors(n: usize) -> Vec<LogEntry> {
    let buf = BUFFER.lock().unwrap();
    buf.iter().rev()
        .filter(|e| matches!(e.level, LogLevel::Error | LogLevel::Critical))
        .take(n)
        .cloned()
        .collect()
}

pub fn export_to_json() -> String {
    let buf = BUFFER.lock().unwrap();
    serde_json::to_string_pretty(&*buf).unwrap_or_default()
}

pub fn clear_buffer() {
    BUFFER.lock().unwrap().clear();
}

pub fn entry_count()    -> u64 { LOG_ENTRIES_TOTAL.load(Ordering::Relaxed) }
pub fn error_count()    -> u64 { LOG_ERRORS_TOTAL.load(Ordering::Relaxed) }
pub fn critical_count() -> u64 { LOG_CRITICALS_TOTAL.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_log_no_panic() {
        info("test_component", "test message");
        assert!(entry_count() > 0);
    }

    #[test]
    fn error_increments_error_count() {
        let before = error_count();
        error("test", "something failed");
        assert!(error_count() > before);
    }

    #[test]
    fn recent_returns_n_entries() {
        for i in 0..5 { info("test", &format!("msg {}", i)); }
        let entries = recent(3);
        assert!(entries.len() <= 3);
    }

    #[test]
    fn recent_errors_filters_correctly() {
        error("test", "error entry");
        let errors = recent_errors(10);
        assert!(!errors.is_empty());
        for e in &errors {
            assert!(matches!(e.level, LogLevel::Error | LogLevel::Critical));
        }
    }

    #[test]
    fn export_to_json_valid() {
        info("test", "export test");
        let json = export_to_json();
        assert!(json.starts_with('['));
    }
}
