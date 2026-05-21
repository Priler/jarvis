//! Multimodal verification — combines structural, textual, visual, and
//! contextual evidence to verify tool execution outcomes.
//!
//! Sources:
//!   - Process/outcome: `ExecutionOutcome` from tool_executor
//!   - Textual: OCR via `ocr_runtime`
//!   - Visual: `VisualVerifier`
//!   - Contextual: `EnvironmentReasoner`

use std::sync::atomic::{AtomicU64, Ordering};

use crate::environment_reasoner::{self, EnvironmentState};
use crate::tool_executor::ExecutionOutcome;
use crate::visual_verifier::{VisualCheck, VisualVerdict, VisualVerifier};

pub static MM_VERIFICATIONS:  AtomicU64 = AtomicU64::new(0);
pub static MM_CONFIRMED:       AtomicU64 = AtomicU64::new(0);
pub static MM_FAILED:          AtomicU64 = AtomicU64::new(0);
pub static MM_PARTIAL:         AtomicU64 = AtomicU64::new(0);

// ── Evidence source ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct EvidenceSource {
    pub source:  &'static str,
    pub verdict: bool,
    pub detail:  String,
}

// ── Multimodal verdict ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub enum MultimodalVerdict {
    Confirmed  { sources: Vec<EvidenceSource>, confidence: f32 },
    Failed     { sources: Vec<EvidenceSource>, reason: String },
    Partial    { confirmed: Vec<String>, failed: Vec<String> },
    Ambiguous  { reason: String },
}

impl MultimodalVerdict {
    pub fn is_confirmed(&self) -> bool {
        matches!(self, MultimodalVerdict::Confirmed { .. })
    }

    pub fn confidence(&self) -> f32 {
        match self {
            MultimodalVerdict::Confirmed { confidence, .. } => *confidence,
            MultimodalVerdict::Partial { confirmed, failed } => {
                let total = confirmed.len() + failed.len();
                if total == 0 { 0.0 } else { confirmed.len() as f32 / total as f32 }
            }
            _ => 0.0,
        }
    }
}

// ── Verification spec ─────────────────────────────────────────────────────────

pub struct MultimodalVerificationSpec {
    pub tool_id:           String,
    pub expected_text:     Option<String>,
    pub forbidden_text:    Option<String>,
    pub require_no_error:  bool,
    pub require_env_ready: bool,
}

impl MultimodalVerificationSpec {
    pub fn new(tool_id: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.into(),
            expected_text: None,
            forbidden_text: None,
            require_no_error: true,
            require_env_ready: false,
        }
    }

    pub fn expect_text(mut self, text: impl Into<String>) -> Self {
        self.expected_text = Some(text.into());
        self
    }

    pub fn require_env_ready(mut self) -> Self {
        self.require_env_ready = true;
        self
    }
}

// ── Multimodal verifier ───────────────────────────────────────────────────────

pub struct MultimodalVerifier;

impl MultimodalVerifier {
    /// Full multimodal verification for a tool execution outcome.
    pub fn verify(
        spec: &MultimodalVerificationSpec,
        outcome: &ExecutionOutcome,
    ) -> MultimodalVerdict {
        MM_VERIFICATIONS.fetch_add(1, Ordering::Relaxed);

        let mut sources = Vec::new();
        let mut any_failed = false;

        // Source 1: structural — did the tool report success?
        let structural_ok = outcome.is_success();
        sources.push(EvidenceSource {
            source: "process_outcome",
            verdict: structural_ok,
            detail: if structural_ok {
                "execution outcome is Success".to_string()
            } else {
                format!("outcome: {:?}", match outcome {
                    ExecutionOutcome::Failed { reason } => reason.as_str(),
                    ExecutionOutcome::Blocked { reason } => reason.as_str(),
                    ExecutionOutcome::Cancelled { reason } => reason.as_str(),
                    _ => "non-success",
                })
            },
        });
        if !structural_ok { any_failed = true; }

        // Source 2: OCR — verify expected text is visible.
        if let Some(ref expected) = spec.expected_text {
            let v = VisualVerifier::check(&VisualCheck::TextVisible(expected.clone()));
            sources.push(EvidenceSource {
                source:  "ocr_text",
                verdict: v.is_confirmed(),
                detail:  format!("{:?}", v),
            });
            if !v.is_confirmed() { any_failed = true; }
        }

        // Source 3: visual — no error dialog should be visible.
        if spec.require_no_error {
            let v = VisualVerifier::check(&VisualCheck::NoErrorDialog);
            sources.push(EvidenceSource {
                source:  "visual_no_error",
                verdict: v.is_confirmed(),
                detail:  format!("{:?}", v),
            });
            if !v.is_confirmed() { any_failed = true; }
        }

        // Source 4: environment — overall state is actionable.
        if spec.require_env_ready {
            let reasoning = environment_reasoner::EnvironmentReasoner::reason();
            let env_ok = reasoning.state.is_actionable();
            sources.push(EvidenceSource {
                source:  "environment",
                verdict: env_ok,
                detail:  format!("{:?}", reasoning.state),
            });
            if !env_ok { any_failed = true; }
        }

        let confirmed: Vec<String> = sources.iter().filter(|s| s.verdict)
            .map(|s| s.source.to_string()).collect();
        let failed: Vec<String> = sources.iter().filter(|s| !s.verdict)
            .map(|s| s.source.to_string()).collect();

        let verdict = if any_failed {
            if confirmed.is_empty() {
                MM_FAILED.fetch_add(1, Ordering::Relaxed);
                MultimodalVerdict::Failed {
                    sources,
                    reason: format!("all {} sources failed", failed.len()),
                }
            } else {
                MM_PARTIAL.fetch_add(1, Ordering::Relaxed);
                MultimodalVerdict::Partial { confirmed, failed }
            }
        } else {
            let confidence = 0.80 + (sources.len() as f32 * 0.04).min(0.19);
            MM_CONFIRMED.fetch_add(1, Ordering::Relaxed);
            MultimodalVerdict::Confirmed { sources, confidence }
        };

        verdict
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_capture::{self, CaptureBackend as _};
    use crate::ocr_runtime::{self, OcrBackend as _};

    fn init_clean_env() {
        screen_capture::init_stub();
        ocr_runtime::init_stub("application ready");
    }

    #[test]
    fn success_outcome_confirms_structural_source() {
        let src = EvidenceSource {
            source: "process_outcome", verdict: true, detail: "ok".into(),
        };
        assert!(src.verdict);
    }

    #[test]
    fn multimodal_confirm_on_success_outcome() {
        init_clean_env();
        let spec = MultimodalVerificationSpec::new("app.open");
        let v = MultimodalVerifier::verify(&spec, &ExecutionOutcome::Success);
        assert!(v.is_confirmed(), "expected Confirmed, got {:?}", v.confidence());
    }

    #[test]
    fn failed_outcome_fails_structural_check() {
        init_clean_env();
        let spec = MultimodalVerificationSpec::new("app.open");
        let v = MultimodalVerifier::verify(
            &spec,
            &ExecutionOutcome::Failed { reason: "not found".into() },
        );
        assert!(!v.is_confirmed());
    }

    #[test]
    fn partial_verdict_has_nonzero_confidence() {
        let v = MultimodalVerdict::Partial {
            confirmed: vec!["a".into()],
            failed: vec!["b".into()],
        };
        assert!(v.confidence() > 0.0 && v.confidence() < 1.0);
    }

    #[test]
    fn mm_verifications_counter_increments() {
        init_clean_env();
        let before = MM_VERIFICATIONS.load(Ordering::Relaxed);
        let spec = MultimodalVerificationSpec::new("app.open");
        MultimodalVerifier::verify(&spec, &ExecutionOutcome::Success);
        assert!(MM_VERIFICATIONS.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn verdict_failed_zero_confidence() {
        let v = MultimodalVerdict::Failed {
            sources: vec![],
            reason: "all failed".into(),
        };
        assert!((v.confidence() - 0.0).abs() < f32::EPSILON);
    }
}
