//! Anomaly detector — identifies pathological desktop states by pattern analysis.
//!
//! Detects: frozen apps, stuck loading screens, popup storms, repeated failures,
//! and unexpected regressions.  All detection is local (OCR + world model history).

use std::sync::atomic::{AtomicU64, Ordering};

pub static ANOMALY_CHECKS:   AtomicU64 = AtomicU64::new(0);
pub static ANOMALIES_FOUND:  AtomicU64 = AtomicU64::new(0);
pub static ANOMALIES_CLEAR:  AtomicU64 = AtomicU64::new(0);

const LOADING_STUCK_MS:   u64 = 15_000;
const POPUP_STORM_COUNT:  u32 = 5;

// ── Anomaly kinds ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AnomalyKind {
    FrozenApp       { process: String },
    StuckLoading    { hint: String, duration_ms: u64 },
    PopupStorm      { count: u32 },
    RepeatedFailure { tool_id: String, count: u32 },
    UnexpectedClose { process: String },
    PermissionLoop  { hint: String },
    CrashLoop       { process: String, count: u32 },
    ScreenLocked,
}

impl AnomalyKind {
    pub fn severity(&self) -> &'static str {
        match self {
            AnomalyKind::CrashLoop { .. }       => "critical",
            AnomalyKind::FrozenApp { .. }        => "high",
            AnomalyKind::PermissionLoop { .. }   => "high",
            AnomalyKind::RepeatedFailure { .. }  => "medium",
            AnomalyKind::StuckLoading { .. }     => "medium",
            AnomalyKind::PopupStorm { .. }        => "medium",
            AnomalyKind::UnexpectedClose { .. }  => "low",
            AnomalyKind::ScreenLocked            => "low",
        }
    }

    pub fn requires_intervention(&self) -> bool {
        matches!(self.severity(), "critical" | "high")
    }

    pub fn label(&self) -> &'static str {
        match self {
            AnomalyKind::FrozenApp { .. }        => "FrozenApp",
            AnomalyKind::StuckLoading { .. }     => "StuckLoading",
            AnomalyKind::PopupStorm { .. }        => "PopupStorm",
            AnomalyKind::RepeatedFailure { .. }  => "RepeatedFailure",
            AnomalyKind::UnexpectedClose { .. }  => "UnexpectedClose",
            AnomalyKind::PermissionLoop { .. }   => "PermissionLoop",
            AnomalyKind::CrashLoop { .. }        => "CrashLoop",
            AnomalyKind::ScreenLocked            => "ScreenLocked",
        }
    }
}

// ── Detected anomaly ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectedAnomaly {
    pub kind:       AnomalyKind,
    pub ts_ms:      u64,
    pub confidence: f32,
    pub evidence:   String,
}

impl DetectedAnomaly {
    pub fn new(kind: AnomalyKind, confidence: f32, evidence: impl Into<String>) -> Self {
        Self { kind, ts_ms: ts_now(), confidence, evidence: evidence.into() }
    }
}

// ── Anomaly detector ──────────────────────────────────────────────────────────

pub struct AnomalyDetector;

impl AnomalyDetector {
    /// Scan current runtime state and return any detected anomalies.
    pub fn scan() -> Vec<DetectedAnomaly> {
        ANOMALY_CHECKS.fetch_add(1, Ordering::Relaxed);
        let mut anomalies = Vec::new();

        // Check 1: environment reasoner for crash/stuck states
        {
            use crate::environment_reasoner::{EnvironmentReasoner, EnvironmentState};
            let reasoning = EnvironmentReasoner::reason();
            match &reasoning.state {
                EnvironmentState::CrashDetected { hint } => {
                    anomalies.push(DetectedAnomaly::new(
                        AnomalyKind::FrozenApp { process: hint.clone() },
                        reasoning.confidence,
                        format!("environment: {:?}", reasoning.state),
                    ));
                }
                EnvironmentState::PermissionRequired { hint } => {
                    anomalies.push(DetectedAnomaly::new(
                        AnomalyKind::PermissionLoop { hint: hint.clone() },
                        reasoning.confidence,
                        "permission dialog detected by environment reasoner".to_string(),
                    ));
                }
                EnvironmentState::InstallerFrozen => {
                    anomalies.push(DetectedAnomaly::new(
                        AnomalyKind::StuckLoading {
                            hint: "installer".to_string(),
                            duration_ms: LOADING_STUCK_MS,
                        },
                        0.75,
                        "installer frozen state detected".to_string(),
                    ));
                }
                _ => {}
            }
        }

        // Check 2: screen locked
        {
            let locked = crate::world_state::with_state(|s| {
                s.snapshot.as_ref().map(|snap| snap.ui_state.is_screen_locked).unwrap_or(false)
            });
            if locked {
                anomalies.push(DetectedAnomaly::new(
                    AnomalyKind::ScreenLocked,
                    1.0,
                    "world state reports screen locked".to_string(),
                ));
            }
        }

        // Check 3: popup storm — multiple blocking dialogs
        {
            let dialog_count = crate::world_state::with_state(|s| {
                s.snapshot.as_ref().map(|snap| snap.ui_state.active_dialogs.len()).unwrap_or(0)
            });
            if dialog_count as u32 >= POPUP_STORM_COUNT {
                anomalies.push(DetectedAnomaly::new(
                    AnomalyKind::PopupStorm { count: dialog_count as u32 },
                    0.85,
                    format!("{} simultaneous dialogs", dialog_count),
                ));
            }
        }

        // Check 4: world model history — loading state persisting too long
        {
            use crate::persistent_world_model;
            let recent = persistent_world_model::recent(5);
            if recent.len() >= 3 {
                let all_loading = recent.iter().all(|e| {
                    e.env_state.to_lowercase().contains("loading") ||
                    e.env_state.to_lowercase().contains("launching")
                });
                if all_loading {
                    let oldest = recent.first().map(|e| e.ts_ms).unwrap_or_else(ts_now);
                    let duration = ts_now().saturating_sub(oldest);
                    if duration >= LOADING_STUCK_MS {
                        anomalies.push(DetectedAnomaly::new(
                            AnomalyKind::StuckLoading {
                                hint: recent.last().and_then(|e| e.active_app.clone())
                                    .unwrap_or_else(|| "unknown".to_string()),
                                duration_ms: duration,
                            },
                            0.70,
                            format!("loading state persisted {}ms", duration),
                        ));
                    }
                }
            }
        }

        if anomalies.is_empty() {
            ANOMALIES_CLEAR.fetch_add(1, Ordering::Relaxed);
        } else {
            ANOMALIES_FOUND.fetch_add(anomalies.len() as u64, Ordering::Relaxed);
        }

        anomalies
    }

    /// True when any critical or high-severity anomaly is present.
    pub fn has_critical_anomaly() -> bool {
        Self::scan().iter().any(|a| a.kind.requires_intervention())
    }
}

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_runs_without_panic() {
        let anomalies = AnomalyDetector::scan();
        // May be empty or contain anomalies depending on runtime state
        drop(anomalies);
    }

    #[test]
    fn anomaly_kind_severity_is_consistent() {
        let a = AnomalyKind::CrashLoop { process: "app".into(), count: 3 };
        assert_eq!(a.severity(), "critical");
        assert!(a.requires_intervention());
    }

    #[test]
    fn screen_locked_is_low_severity() {
        let a = AnomalyKind::ScreenLocked;
        assert_eq!(a.severity(), "low");
        assert!(!a.requires_intervention());
    }

    #[test]
    fn detected_anomaly_has_timestamp() {
        let a = DetectedAnomaly::new(AnomalyKind::ScreenLocked, 1.0, "test");
        assert!(a.ts_ms > 0);
    }

    #[test]
    fn anomaly_checks_counter_increments() {
        let before = ANOMALY_CHECKS.load(Ordering::Relaxed);
        AnomalyDetector::scan();
        assert!(ANOMALY_CHECKS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn anomaly_label_is_non_empty() {
        let kinds = [
            AnomalyKind::FrozenApp { process: "x".into() },
            AnomalyKind::PopupStorm { count: 5 },
            AnomalyKind::ScreenLocked,
        ];
        for k in &kinds {
            assert!(!k.label().is_empty());
        }
    }
}
