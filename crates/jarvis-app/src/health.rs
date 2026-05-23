#![allow(dead_code)]

//! Runtime health scoring system.
//!
//! `RuntimeHealth::compute()` samples all available runtime atomics and produces
//! a 0–100 score per subsystem plus an overall score.  The result can be logged
//! (via `log()`) or serialised as a JSON line (via `to_json()`).
//!
//! Called by the watchdog every 60 s and on demand.

use std::sync::atomic::Ordering;

use crate::stt_worker::{
    FORCED_GATE_RESETS, LAST_IPC_ACTIVITY_MS, RECOVERY_FAILED, RECOVERY_TOTAL,
    RECOGNIZER_REBUILDS,
};
use crate::watchdog::{DEGRADED_MODE, WATCHDOG_INCIDENTS};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct RuntimeHealth {
    pub audio_score: u8,
    pub stt_score: u8,
    pub ipc_score: u8,
    pub runtime_score: u8,
    pub degraded: bool,
    pub incidents: u64,
    pub ts_ms: u64,
}

impl RuntimeHealth {
    pub fn compute() -> Self {
        let degraded = DEGRADED_MODE.load(Ordering::Relaxed);
        let now = now_ms();

        // Audio: penalise for each forced gate reset (10 pts each, max 100 penalty).
        let gate_resets = FORCED_GATE_RESETS.load(Ordering::Relaxed);
        let audio_score: u8 = if degraded {
            0
        } else {
            100u8.saturating_sub((gate_resets.min(10) * 10) as u8)
        };

        // STT: penalise for failed recoveries and recognizer full-rebuilds.
        let recovery_total = RECOVERY_TOTAL.load(Ordering::Relaxed);
        let recovery_failed = RECOVERY_FAILED.load(Ordering::Relaxed);
        let rebuilds = RECOGNIZER_REBUILDS.load(Ordering::Relaxed);
        let stt_score: u8 = if degraded {
            0
        } else if recovery_total == 0 {
            100
        } else {
            let failure_pct = ((recovery_failed * 100) / recovery_total.max(1)) as u8;
            100u8.saturating_sub(failure_pct)
                  .saturating_sub((rebuilds.min(5) * 5) as u8)
        };

        // IPC: degrade based on time since last activity.
        let last_ipc = LAST_IPC_ACTIVITY_MS.load(Ordering::Relaxed);
        let ipc_score: u8 = if last_ipc == 0 {
            75 // no GUI connected yet — neutral
        } else {
            let age_s = now.saturating_sub(last_ipc) / 1000;
            match age_s {
                0..=29 => 100,
                30..=59 => 90,
                60..=119 => 75,
                120..=299 => 50,
                _ => 25,
            }
        };

        let runtime_score: u8 = if degraded {
            0
        } else {
            ((audio_score as u32 + stt_score as u32 + ipc_score as u32) / 3) as u8
        };

        RuntimeHealth {
            audio_score,
            stt_score,
            ipc_score,
            runtime_score,
            degraded,
            incidents: WATCHDOG_INCIDENTS.load(Ordering::Relaxed),
            ts_ms: now,
        }
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"ts\":{},\"score\":{},\"audio\":{},\"stt\":{},\"ipc\":{},\"degraded\":{},\"incidents\":{}}}",
            self.ts_ms,
            self.runtime_score,
            self.audio_score,
            self.stt_score,
            self.ipc_score,
            self.degraded,
            self.incidents,
        )
    }

    pub fn log(&self) {
        if self.degraded {
            error!(
                "[HEALTH] DEGRADED audio={} stt={} ipc={} incidents={}",
                self.audio_score, self.stt_score, self.ipc_score, self.incidents
            );
        } else {
            info!(
                "[HEALTH] score={} audio={} stt={} ipc={} incidents={}",
                self.runtime_score, self.audio_score, self.stt_score, self.ipc_score,
                self.incidents
            );
        }
    }
}
