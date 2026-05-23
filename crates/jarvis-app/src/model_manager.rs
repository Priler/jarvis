//! Model manager — detects installed Ollama models, estimates VRAM requirements,
//! recommends runtime profiles. Uses raw TcpStream (no new dependencies).

use std::sync::{Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use once_cell::sync::Lazy;

pub static MODELS_DETECTED: AtomicU64 = AtomicU64::new(0);
pub static LAST_SCAN_MS:    AtomicU64 = AtomicU64::new(0);
static OLLAMA_AVAILABLE:    AtomicBool = AtomicBool::new(false);

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ModelFamily {
    Qwen, Llama, DeepSeek, Gemma, Phi, Mistral, Unknown,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum RuntimeProfile { Lite, Standard, Full }

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub name:             String,
    pub family:           ModelFamily,
    pub size_gb:          f32,
    pub vram_required_gb: f32,
    pub profile:          RuntimeProfile,
}

static MODEL_LIST: Lazy<Mutex<Vec<ModelInfo>>> = Lazy::new(|| Mutex::new(Vec::new()));

// ── Internal helpers ──────────────────────────────────────────────────────────

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn detect_family(name: &str) -> ModelFamily {
    let l = name.to_lowercase();
    if l.contains("qwen")     { ModelFamily::Qwen }
    else if l.contains("deepseek") { ModelFamily::DeepSeek }
    else if l.contains("llama")    { ModelFamily::Llama }
    else if l.contains("gemma")    { ModelFamily::Gemma }
    else if l.contains("phi")      { ModelFamily::Phi }
    else if l.contains("mistral")  { ModelFamily::Mistral }
    else                           { ModelFamily::Unknown }
}

fn estimate_vram_gb(name: &str) -> f32 {
    let l = name.to_lowercase();
    if      l.contains("70b") || l.contains("72b") { 42.0 }
    else if l.contains("34b") || l.contains("32b") { 20.0 }
    else if l.contains("13b") || l.contains("14b") {  8.0 }
    else if l.contains("7b")  || l.contains("8b")  {  4.5 }
    else if l.contains("3b")  || l.contains("4b")  {  2.5 }
    else if l.contains("1b")                       {  1.0 }
    else                                           {  4.0 }
}

fn profile_from_vram(vram: f32) -> RuntimeProfile {
    if vram > 16.0 { RuntimeProfile::Full }
    else if vram >= 8.0 { RuntimeProfile::Standard }
    else { RuntimeProfile::Lite }
}

/// Parse model names from Ollama's /api/tags JSON body.
fn parse_names_from_json(body: &str) -> Vec<String> {
    let mut names = Vec::new();
    // Split on "name":" and collect until next quote — works without a JSON parser.
    for segment in body.split("\"name\":\"") {
        if let Some(end) = segment.find('"') {
            let candidate = segment[..end].trim();
            if !candidate.is_empty() && candidate != "models" && candidate.len() > 1 {
                names.push(candidate.to_string());
            }
        }
    }
    names
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Scan Ollama at localhost:11434 and return detected models.
/// Returns empty if Ollama is not running.
pub fn scan_ollama() -> Vec<ModelInfo> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    LAST_SCAN_MS.store(ts_now(), Ordering::Relaxed);

    let mut stream = match TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().unwrap(),
        Duration::from_millis(500),
    ) {
        Ok(s)  => { OLLAMA_AVAILABLE.store(true, Ordering::Relaxed); s }
        Err(_) => { OLLAMA_AVAILABLE.store(false, Ordering::Relaxed); return Vec::new(); }
    };

    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.write_all(
        b"GET /api/tags HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );

    let mut body = String::new();
    let _ = stream.read_to_string(&mut body);

    let names = parse_names_from_json(&body);
    let models: Vec<ModelInfo> = names.into_iter().map(|name| {
        let vram = estimate_vram_gb(&name);
        ModelInfo {
            size_gb: vram * 0.6,
            vram_required_gb: vram,
            profile: profile_from_vram(vram),
            family: detect_family(&name),
            name,
        }
    }).collect();

    MODELS_DETECTED.store(models.len() as u64, Ordering::Relaxed);
    *MODEL_LIST.lock().unwrap() = models.clone();
    models
}

pub fn is_ollama_available() -> bool { OLLAMA_AVAILABLE.load(Ordering::Relaxed) }
pub fn get_cached()           -> Vec<ModelInfo> { MODEL_LIST.lock().unwrap().clone() }
pub fn models_detected()      -> u64  { MODELS_DETECTED.load(Ordering::Relaxed) }
pub fn last_scan_ms()         -> u64  { LAST_SCAN_MS.load(Ordering::Relaxed) }

/// Shell command needed to download a model.
pub fn download_instruction(model_name: &str) -> String {
    format!("ollama pull {}", model_name)
}

/// Recommended profile based on the largest available model.
pub fn recommend_profile() -> RuntimeProfile {
    let list = MODEL_LIST.lock().unwrap();
    if list.iter().any(|m| m.profile == RuntimeProfile::Full)     { RuntimeProfile::Full }
    else if list.iter().any(|m| m.profile == RuntimeProfile::Standard) { RuntimeProfile::Standard }
    else { RuntimeProfile::Lite }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_family_all_families() {
        assert_eq!(detect_family("qwen2.5:7b"),    ModelFamily::Qwen);
        assert_eq!(detect_family("llama3:8b"),     ModelFamily::Llama);
        assert_eq!(detect_family("deepseek-r1"),   ModelFamily::DeepSeek);
        assert_eq!(detect_family("gemma2:9b"),     ModelFamily::Gemma);
        assert_eq!(detect_family("phi3:mini"),     ModelFamily::Phi);
        assert_eq!(detect_family("mistral:7b"),    ModelFamily::Mistral);
        assert_eq!(detect_family("unknown-model"), ModelFamily::Unknown);
    }

    #[test]
    fn estimate_vram_bounds() {
        assert!(estimate_vram_gb("llama3:7b") > 3.0);
        assert!(estimate_vram_gb("llama3:70b") > 30.0);
        assert!(estimate_vram_gb("phi3:1b") < 2.0);
    }

    #[test]
    fn profile_from_vram_correct() {
        assert_eq!(profile_from_vram(4.0), RuntimeProfile::Lite);
        assert_eq!(profile_from_vram(12.0), RuntimeProfile::Standard);
        assert_eq!(profile_from_vram(40.0), RuntimeProfile::Full);
    }

    #[test]
    fn scan_ollama_no_panic() {
        let models = scan_ollama();
        // Either empty (Ollama not running) or non-empty (running)
        assert!(models.len() < 1000);
    }

    #[test]
    fn download_instruction_contains_pull() {
        assert!(download_instruction("qwen2.5:7b").contains("ollama pull"));
    }

    #[test]
    fn parse_names_from_empty_body() {
        let names = parse_names_from_json("{}");
        assert!(names.is_empty());
    }
}
