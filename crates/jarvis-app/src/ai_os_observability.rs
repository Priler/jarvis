//! AI OS observability — event log for kernel, distributed runtime, recursive
//! orchestrator, scheduler, recovery, persistent services, and process manager.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;

pub static EVENTS_RECORDED: AtomicU64 = AtomicU64::new(0);

const MAX_LOG: usize = 500;

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── AiOsEvent ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AiOsEvent {
    KernelTick          { tick_id: u64, steps: usize },
    ServiceStarted      { name: String },
    ServiceStopped      { name: String, reason: String },
    ServiceRecovered    { name: String },
    RecursionDepth      { depth: u32, safe: bool },
    DistributedRebalance{ workers: usize, avg_load: f32 },
    RecoveryAction      { component: String, action: String },
    SchedulerDecision   { job: String, priority: f32 },
    SafetyGate          { component: String, reason: String },
    ResourceThrottle    { component: String, pressure: f32 },
}

impl AiOsEvent {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::KernelTick          { .. } => "[KERNEL]",
            Self::ServiceStarted      { .. } => "[PERSISTENT]",
            Self::ServiceStopped      { .. } => "[SERVICES]",
            Self::ServiceRecovered    { .. } => "[RECOVERY]",
            Self::RecursionDepth      { .. } => "[RECURSIVE]",
            Self::DistributedRebalance{ .. } => "[DISTRIBUTED]",
            Self::RecoveryAction      { .. } => "[RECOVERY]",
            Self::SchedulerDecision   { .. } => "[SCHEDULER]",
            Self::SafetyGate          { .. } => "[KERNEL]",
            Self::ResourceThrottle    { .. } => "[DISTRIBUTED]",
        }
    }

    pub fn severity(&self) -> u8 {
        match self {
            Self::SafetyGate    { .. } => 3,
            Self::RecoveryAction{ .. } => 2,
            Self::ServiceStopped{ .. } => 2,
            Self::ResourceThrottle{..} => 1,
            _                          => 0,
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

static LOG: Lazy<Mutex<Vec<(u64, AiOsEvent)>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── API ───────────────────────────────────────────────────────────────────────

pub fn record(event: AiOsEvent) {
    let ts = ts_now();
    let mut log = LOG.lock().unwrap();
    if log.len() >= MAX_LOG { log.remove(0); }
    log.push((ts, event));
    EVENTS_RECORDED.fetch_add(1, Ordering::Relaxed);
}

pub fn recent(n: usize) -> Vec<(u64, AiOsEvent)> {
    LOG.lock().unwrap().iter().rev().take(n).cloned().collect()
}

pub fn event_count() -> usize {
    LOG.lock().unwrap().len()
}

pub fn recovery_actions_count() -> usize {
    LOG.lock().unwrap().iter()
        .filter(|(_, e)| matches!(e, AiOsEvent::RecoveryAction { .. }))
        .count()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        record(AiOsEvent::KernelTick { tick_id: 1, steps: 10 });
        assert!(event_count() > 0);
        let r = recent(1);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn severity_safety_gate_is_high() {
        let e = AiOsEvent::SafetyGate {
            component: "test".into(), reason: "test".into()
        };
        assert_eq!(e.severity(), 3);
    }
}
