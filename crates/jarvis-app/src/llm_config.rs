//! LLM backend configuration.
//!
//! `LlmConfig` selects which local inference backend to use and carries
//! all tunable parameters.  Default is the `Stub` backend — runtime never
//! calls a real model unless explicitly configured.
//!
//! Offline guarantee: `offline_only` MUST remain `true`.  Any implementation
//! that calls a remote URL will be rejected by `validate()`.

// ── Backend selection ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LlmBackend {
    /// In-process stub — deterministic, never blocks, always available.
    Stub,
    /// Local Ollama server on `endpoint` (default http://127.0.0.1:11434).
    Ollama,
    /// llama.cpp binary invoked as a subprocess.
    LlamaCpp,
}

impl LlmBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmBackend::Stub     => "stub",
            LlmBackend::Ollama   => "ollama",
            LlmBackend::LlamaCpp => "llama_cpp",
        }
    }
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Runtime configuration for a local LLM backend.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LlmConfig {
    pub backend:      LlmBackend,
    /// Model identifier (e.g. "llama3", "mistral", path for LlamaCpp).
    pub model:        String,
    /// For Ollama: base URL of the local server.  MUST be localhost.
    pub endpoint:     String,
    /// For LlamaCpp: path to the compiled binary.
    pub binary_path:  Option<String>,
    pub context_len:  usize,
    /// Sampling temperature — 0.0 = deterministic, 1.0 = creative.
    pub temperature:  f32,
    pub max_tokens:   u32,
    pub timeout_ms:   u64,
    /// Safety flag: if true, any non-localhost endpoint is rejected.
    pub offline_only: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            backend:      LlmBackend::Stub,
            model:        "stub".into(),
            endpoint:     "http://127.0.0.1:11434".into(),
            binary_path:  None,
            context_len:  2048,
            temperature:  0.0,
            max_tokens:   256,
            timeout_ms:   2000,
            offline_only: true,
        }
    }
}

impl LlmConfig {
    /// Preset for local Ollama with a specified model.
    pub fn ollama(model: impl Into<String>) -> Self {
        Self {
            backend:  LlmBackend::Ollama,
            model:    model.into(),
            ..Default::default()
        }
    }

    /// Preset for llama.cpp subprocess with a binary and model path.
    pub fn llamacpp(binary: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            backend:     LlmBackend::LlamaCpp,
            binary_path: Some(binary.into()),
            model:       model.into(),
            ..Default::default()
        }
    }

    /// Returns true if the endpoint is localhost (required for offline guarantee).
    pub fn endpoint_is_local(&self) -> bool {
        let ep = &self.endpoint;
        ep.contains("127.0.0.1") || ep.contains("localhost") || ep.contains("::1")
    }

    /// Validate config — returns error string if unsafe.
    pub fn validate(&self) -> Result<(), String> {
        if self.offline_only && !self.endpoint_is_local()
            && self.backend == LlmBackend::Ollama
        {
            return Err(format!(
                "LlmConfig: offline_only=true but endpoint '{}' is not localhost",
                self.endpoint
            ));
        }
        if self.temperature < 0.0 || self.temperature > 1.0 {
            return Err(format!(
                "LlmConfig: temperature {} out of range [0.0, 1.0]",
                self.temperature
            ));
        }
        if self.max_tokens == 0 {
            return Err("LlmConfig: max_tokens must be > 0".into());
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_stub_backend() {
        assert_eq!(LlmConfig::default().backend, LlmBackend::Stub);
    }

    #[test]
    fn default_is_offline_only() {
        assert!(LlmConfig::default().offline_only);
    }

    #[test]
    fn default_endpoint_is_local() {
        assert!(LlmConfig::default().endpoint_is_local());
    }

    #[test]
    fn temperature_out_of_range_fails_validation() {
        let mut cfg = LlmConfig::default();
        cfg.temperature = 2.0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn valid_default_config_passes() {
        assert!(LlmConfig::default().validate().is_ok());
    }
}
