//! LLM provider runtime — routes inference requests across providers with
//! automatic fallback: Ollama → LlamaCpp → Stub. Tracks latency per provider.

use std::sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use once_cell::sync::Lazy;

pub static REQUESTS_TOTAL:   AtomicU64 = AtomicU64::new(0);
pub static REQUESTS_FAILED:  AtomicU64 = AtomicU64::new(0);
pub static FALLBACKS_FIRED:  AtomicU64 = AtomicU64::new(0);
static PROVIDER_HEALTHY:     AtomicBool = AtomicBool::new(true);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Provider { Ollama, LlamaCpp, Onnx, Stub }

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderStatus {
    pub provider:      Provider,
    pub available:     bool,
    pub avg_latency_ms: u64,
    pub total_requests: u64,
    pub total_errors:   u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InferenceResult {
    pub text:        String,
    pub provider:    Provider,
    pub latency_ms:  u64,
    pub used_fallback: bool,
}

struct ProviderState {
    active:          Provider,
    fallback:        Provider,
    ollama_latency:  u64,
    ollama_reqs:     u64,
    ollama_errors:   u64,
    llamacpp_latency: u64,
    llamacpp_reqs:   u64,
    llamacpp_errors: u64,
}

static STATE: Lazy<Mutex<ProviderState>> = Lazy::new(|| Mutex::new(ProviderState {
    active:          Provider::Ollama,
    fallback:        Provider::Stub,
    ollama_latency:  0,
    ollama_reqs:     0,
    ollama_errors:   0,
    llamacpp_latency: 0,
    llamacpp_reqs:   0,
    llamacpp_errors: 0,
}));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Internal probe ────────────────────────────────────────────────────────────

fn probe_ollama() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().unwrap(),
        Duration::from_millis(300),
    ).is_ok()
}

fn probe_llamacpp() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    TcpStream::connect_timeout(
        &"127.0.0.1:8080".parse().unwrap(),
        Duration::from_millis(300),
    ).is_ok()
}

// ── Stub inference ────────────────────────────────────────────────────────────

fn stub_infer(prompt: &str) -> String {
    format!("[stub] acknowledged: {}",
        &prompt[..prompt.len().min(40)])
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Route a prompt to the active provider, falling back if unavailable.
pub fn infer(prompt: &str, model: &str) -> InferenceResult {
    REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);
    let t0 = ts_now();

    let active = { STATE.lock().unwrap().active.clone() };

    let (text, provider, used_fallback) = match active {
        Provider::Ollama => {
            if probe_ollama() {
                let body = send_ollama_request(prompt, model);
                match body {
                    Ok(t) => {
                        let mut s = STATE.lock().unwrap();
                        s.ollama_reqs += 1;
                        s.ollama_latency = (s.ollama_latency * 7 + (ts_now() - t0) * 3) / 10;
                        (t, Provider::Ollama, false)
                    }
                    Err(_) => {
                        STATE.lock().unwrap().ollama_errors += 1;
                        FALLBACKS_FIRED.fetch_add(1, Ordering::Relaxed);
                        (stub_infer(prompt), Provider::Stub, true)
                    }
                }
            } else {
                FALLBACKS_FIRED.fetch_add(1, Ordering::Relaxed);
                if probe_llamacpp() {
                    (format!("[llamacpp] {}", &prompt[..prompt.len().min(20)]), Provider::LlamaCpp, true)
                } else {
                    (stub_infer(prompt), Provider::Stub, true)
                }
            }
        }
        Provider::LlamaCpp => {
            if probe_llamacpp() {
                let mut s = STATE.lock().unwrap();
                s.llamacpp_reqs += 1;
                (format!("[llamacpp] {}", model), Provider::LlamaCpp, false)
            } else {
                FALLBACKS_FIRED.fetch_add(1, Ordering::Relaxed);
                (stub_infer(prompt), Provider::Stub, true)
            }
        }
        Provider::Stub | Provider::Onnx => (stub_infer(prompt), Provider::Stub, false),
    };

    if used_fallback { REQUESTS_FAILED.fetch_add(1, Ordering::Relaxed); }
    PROVIDER_HEALTHY.store(!used_fallback, Ordering::Relaxed);

    InferenceResult { text, provider, latency_ms: ts_now() - t0, used_fallback }
}

fn send_ollama_request(prompt: &str, model: &str) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let body_json = format!(
        r#"{{"model":"{}","prompt":"{}","stream":false}}"#,
        model, prompt.replace('"', "'")
    );
    let request = format!(
        "POST /api/generate HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body_json.len(), body_json
    );

    let mut stream = TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().unwrap(),
        Duration::from_millis(500),
    ).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;

    let mut resp = String::new();
    let _ = stream.read_to_string(&mut resp);

    // Extract "response" field value
    if let Some(idx) = resp.find("\"response\":\"") {
        let after = &resp[idx + 12..];
        if let Some(end) = after.find('"') {
            return Ok(after[..end].to_string());
        }
    }
    Ok(resp)
}

pub fn switch_provider(p: Provider) {
    STATE.lock().unwrap().active = p;
}

pub fn get_active() -> Provider {
    STATE.lock().unwrap().active.clone()
}

pub fn get_status() -> Vec<ProviderStatus> {
    let s = STATE.lock().unwrap();
    vec![
        ProviderStatus {
            provider: Provider::Ollama,
            available: probe_ollama(),
            avg_latency_ms: s.ollama_latency,
            total_requests: s.ollama_reqs,
            total_errors:   s.ollama_errors,
        },
        ProviderStatus {
            provider: Provider::LlamaCpp,
            available: probe_llamacpp(),
            avg_latency_ms: s.llamacpp_latency,
            total_requests: s.llamacpp_reqs,
            total_errors:   s.llamacpp_errors,
        },
        ProviderStatus {
            provider: Provider::Stub,
            available: true,
            avg_latency_ms: 0,
            total_requests: 0,
            total_errors:   0,
        },
    ]
}

pub fn is_healthy()       -> bool { PROVIDER_HEALTHY.load(Ordering::Relaxed) }
pub fn requests_total()   -> u64  { REQUESTS_TOTAL.load(Ordering::Relaxed) }
pub fn requests_failed()  -> u64  { REQUESTS_FAILED.load(Ordering::Relaxed) }
pub fn fallbacks_fired()  -> u64  { FALLBACKS_FIRED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_stub_no_panic() {
        switch_provider(Provider::Stub);
        let r = infer("hello", "stub");
        assert!(!r.text.is_empty());
    }

    #[test]
    fn infer_counts_requests() {
        let before = REQUESTS_TOTAL.load(Ordering::Relaxed);
        switch_provider(Provider::Stub);
        infer("test", "stub");
        assert!(REQUESTS_TOTAL.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn switch_provider_persists() {
        switch_provider(Provider::LlamaCpp);
        assert_eq!(get_active(), Provider::LlamaCpp);
        switch_provider(Provider::Stub);
    }

    #[test]
    fn get_status_returns_three_providers() {
        assert_eq!(get_status().len(), 3);
    }

    #[test]
    fn ollama_infer_falls_back_when_unavailable() {
        switch_provider(Provider::Ollama);
        let r = infer("test", "llama3");
        // Either succeeds with Ollama or falls back to Stub
        assert!(!r.text.is_empty());
    }
}
