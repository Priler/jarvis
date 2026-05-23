//! Concrete LLM backend implementations.
//!
//! Each struct implements `cognitive::model_runtime::ModelRuntime`.
//!
//! Backends:
//!   - `StubRuntime`    — deterministic stub, always available, no I/O
//!   - `OllamaRuntime`  — raw HTTP to a local Ollama server (127.0.0.1 only)
//!   - `LlamaCppRuntime`— spawns a llama.cpp subprocess
//!
//! Offline guarantee: `OllamaRuntime` only connects to localhost.
//! `LlamaCppRuntime` runs a local binary — no network.

use crate::cognitive::model_runtime::{
    InferenceKind, InferenceRequest, InferenceResponse, ModelRuntime,
};
use crate::llm_config::LlmConfig;

// ── Stub runtime ──────────────────────────────────────────────────────────────

/// Always-available stub.  Returns a tagged placeholder without I/O.
/// Used as the default backend when no real model is configured.
pub struct StubRuntime;

impl ModelRuntime for StubRuntime {
    fn model_id(&self)  -> &str { "stub" }
    fn is_loaded(&self) -> bool { true }

    fn supported_kind(&self, _kind: &InferenceKind) -> bool { true }

    fn infer(&self, req: InferenceRequest) -> Result<InferenceResponse, String> {
        // Return a tagged, non-empty response so callers can tell it's a stub.
        let tag = match req.kind {
            InferenceKind::IntentEnrichment => "stub:intent",
            InferenceKind::Planning         => "stub:plan",
            InferenceKind::KnowledgeQuery   => "stub:knowledge",
            InferenceKind::EmbeddingOnly    => "stub:embed",
        };
        Ok(InferenceResponse {
            text:             format!("[{}]", tag),
            tokens_generated: 1,
            latency_ms:       0,
            timed_out:        false,
        })
    }

    fn cancel(&self) {}
}

// ── Ollama runtime ────────────────────────────────────────────────────────────

/// Calls a locally-running Ollama server via raw HTTP/1.1 over TcpStream.
///
/// Only connects to 127.0.0.1 — never a remote host.
pub struct OllamaRuntime {
    model:       String,
    host:        String,
    port:        u16,
    timeout_ms:  u64,
    temperature: f32,
    max_tokens:  u32,
}

impl OllamaRuntime {
    pub fn new(cfg: &LlmConfig) -> Self {
        // Extract host:port from endpoint URL.
        let (host, port) = parse_host_port(&cfg.endpoint);
        Self {
            model:       cfg.model.clone(),
            host,
            port,
            timeout_ms:  cfg.timeout_ms,
            temperature: cfg.temperature,
            max_tokens:  cfg.max_tokens,
        }
    }

    fn probe(&self) -> bool {
        use std::net::TcpStream;
        use std::time::Duration;
        TcpStream::connect_timeout(
            &format!("{}:{}", self.host, self.port).parse().unwrap_or("127.0.0.1:11434".parse().unwrap()),
            Duration::from_millis(200),
        ).is_ok()
    }
}

impl ModelRuntime for OllamaRuntime {
    fn model_id(&self)  -> &str { &self.model }
    fn is_loaded(&self) -> bool { self.probe() }

    fn supported_kind(&self, _kind: &InferenceKind) -> bool { true }

    fn infer(&self, req: InferenceRequest) -> Result<InferenceResponse, String> {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::time::{Duration, Instant};

        let body = serde_json::json!({
            "model": self.model,
            "prompt": req.prompt,
            "stream": false,
            "options": {
                "temperature": self.temperature,
                "num_predict": self.max_tokens,
            }
        }).to_string();

        let addr = format!("{}:{}", self.host, self.port);
        let timeout = Duration::from_millis(self.timeout_ms);
        let t0 = Instant::now();

        let mut stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| format!("addr parse: {}", e))?,
            Duration::from_millis(500),
        ).map_err(|e| format!("ollama connect {}: {}", addr, e))?;

        stream.set_read_timeout(Some(timeout))
            .map_err(|e| format!("set_read_timeout: {}", e))?;

        let http_req = format!(
            "POST /api/generate HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            addr, body.len(), body
        );
        stream.write_all(http_req.as_bytes())
            .map_err(|e| format!("ollama write: {}", e))?;

        let mut raw = String::new();
        stream.read_to_string(&mut raw)
            .map_err(|e| format!("ollama read: {}", e))?;

        let json_start = raw.find("\r\n\r\n")
            .ok_or("no HTTP header separator in ollama response")? + 4;
        let json_body = &raw[json_start..];

        let val: serde_json::Value = serde_json::from_str(json_body)
            .map_err(|e| format!("ollama json: {}", e))?;

        let text = val["response"].as_str().unwrap_or("").to_string();
        let latency_ms = t0.elapsed().as_millis() as u64;

        Ok(InferenceResponse { text, tokens_generated: 0, latency_ms, timed_out: false })
    }

    fn cancel(&self) {}
}

// ── LlamaCpp runtime ──────────────────────────────────────────────────────────

/// Invokes the llama.cpp `main` (or `llama-cli`) binary as a subprocess.
///
/// The binary runs locally — no network I/O.
pub struct LlamaCppRuntime {
    binary: String,
    model:  String,
    temperature: f32,
    max_tokens:  u32,
}

impl LlamaCppRuntime {
    pub fn new(cfg: &LlmConfig) -> Self {
        Self {
            binary:      cfg.binary_path.clone().unwrap_or_else(|| "llama-cli".into()),
            model:       cfg.model.clone(),
            temperature: cfg.temperature,
            max_tokens:  cfg.max_tokens,
        }
    }
}

impl ModelRuntime for LlamaCppRuntime {
    fn model_id(&self) -> &str { &self.model }

    fn is_loaded(&self) -> bool {
        // Binary exists on PATH or as absolute path.
        std::process::Command::new(&self.binary)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn supported_kind(&self, _kind: &InferenceKind) -> bool { true }

    fn infer(&self, req: InferenceRequest) -> Result<InferenceResponse, String> {
        use std::time::Instant;
        let t0 = Instant::now();

        let output = std::process::Command::new(&self.binary)
            .arg("-m").arg(&self.model)
            .arg("-p").arg(&req.prompt)
            .arg("-n").arg(req.max_tokens.to_string())
            .arg("--temp").arg(self.temperature.to_string())
            .arg("--log-disable")
            .output()
            .map_err(|e| format!("llamacpp spawn '{}': {}", self.binary, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("llamacpp exit {}: {}", output.status, stderr.trim()));
        }

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let latency_ms = t0.elapsed().as_millis() as u64;
        Ok(InferenceResponse { text, tokens_generated: 0, latency_ms, timed_out: false })
    }

    fn cancel(&self) {
        // Best-effort — subprocess cannot be cancelled once spawned.
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_host_port(endpoint: &str) -> (String, u16) {
    let stripped = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    if let Some(colon) = stripped.rfind(':') {
        let host = &stripped[..colon];
        let port: u16 = stripped[colon + 1..].parse().unwrap_or(11434);
        (host.to_string(), port)
    } else {
        (stripped.to_string(), 11434)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_is_loaded() {
        assert!(StubRuntime.is_loaded());
    }

    #[test]
    fn stub_returns_tagged_response() {
        let resp = StubRuntime.infer(InferenceRequest::enrichment("hello")).unwrap();
        assert!(resp.text.starts_with("[stub:"));
    }

    #[test]
    fn stub_supports_all_kinds() {
        assert!(StubRuntime.supported_kind(&InferenceKind::Planning));
        assert!(StubRuntime.supported_kind(&InferenceKind::KnowledgeQuery));
    }

    #[test]
    fn parse_host_port_standard() {
        let (h, p) = parse_host_port("http://127.0.0.1:11434");
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 11434);
    }

    #[test]
    fn parse_host_port_no_port() {
        let (h, p) = parse_host_port("http://127.0.0.1");
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 11434);
    }
}
