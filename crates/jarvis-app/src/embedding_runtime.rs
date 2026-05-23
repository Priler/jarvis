//! Embedding runtime — generates text embeddings for semantic search.
//!
//! Primary: Ollama /api/embeddings endpoint (nomic-embed-text or similar).
//! Fallback: TF-IDF bag-of-words vector (no external deps).
//!
//! Vectors are normalized to unit length for cosine similarity.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::collections::HashMap;

pub static EMBEDDINGS_GENERATED: AtomicU64 = AtomicU64::new(0);
pub static EMBEDDINGS_CACHED:    AtomicU64 = AtomicU64::new(0);
static OLLAMA_EMBED_AVAILABLE:   AtomicBool = AtomicBool::new(false);

const EMBED_DIM: usize = 128;
const CACHE_MAX: usize = 256;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Embedding {
    pub vector: Vec<f32>,
    pub source: EmbedSource,
}

#[derive(Clone, PartialEq, Debug)]
pub enum EmbedSource { Ollama, TfIdf }

// ── LRU-style cache ───────────────────────────────────────────────────────────

use std::sync::Mutex;
use once_cell::sync::Lazy;

static CACHE: Lazy<Mutex<HashMap<String, Embedding>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn cache_get(text: &str) -> Option<Embedding> {
    CACHE.lock().unwrap().get(text).cloned()
}

fn cache_put(text: &str, emb: Embedding) {
    let mut cache = CACHE.lock().unwrap();
    if cache.len() >= CACHE_MAX {
        // Evict arbitrary entry
        if let Some(k) = cache.keys().next().cloned() { cache.remove(&k); }
    }
    cache.insert(text.to_string(), emb);
}

// ── TF-IDF fallback ───────────────────────────────────────────────────────────

fn tfidf_embed(text: &str) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBED_DIM];
    for (i, word) in text.split_whitespace().enumerate() {
        let hash = word.bytes().fold(5381u32, |h, b| h.wrapping_mul(33).wrapping_add(b as u32));
        let idx = (hash as usize) % EMBED_DIM;
        vec[idx] += 1.0 / (1.0 + i as f32).sqrt();
    }
    normalize(&mut vec);
    vec
}

fn normalize(v: &mut Vec<f32>) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 { for x in v.iter_mut() { *x /= norm; } }
}

// ── Ollama embedding call ─────────────────────────────────────────────────────

fn ollama_embed(text: &str, model: &str) -> Option<Vec<f32>> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let body = format!(r#"{{"model":"{}","prompt":"{}"}}"#,
        model, text.replace('"', " "));
    let request = format!(
        "POST /api/embeddings HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );

    let mut stream = TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().unwrap(),
        Duration::from_millis(400),
    ).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    stream.write_all(request.as_bytes()).ok()?;

    let mut resp = String::new();
    let _ = stream.read_to_string(&mut resp);
    OLLAMA_EMBED_AVAILABLE.store(true, Ordering::Relaxed);

    // Parse "embedding":[...] array
    if let Some(start) = resp.find("\"embedding\":[") {
        let after = &resp[start + 13..];
        if let Some(end) = after.find(']') {
            let nums: Vec<f32> = after[..end]
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if !nums.is_empty() {
                let mut v = nums;
                normalize(&mut v);
                return Some(v);
            }
        }
    }
    None
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Generate embedding for text. Uses Ollama if available, TF-IDF otherwise.
pub fn embed(text: &str) -> Embedding {
    if let Some(cached) = cache_get(text) {
        EMBEDDINGS_CACHED.fetch_add(1, Ordering::Relaxed);
        return cached;
    }

    let emb = if let Some(v) = ollama_embed(text, "nomic-embed-text") {
        Embedding { vector: v, source: EmbedSource::Ollama }
    } else {
        OLLAMA_EMBED_AVAILABLE.store(false, Ordering::Relaxed);
        Embedding { vector: tfidf_embed(text), source: EmbedSource::TfIdf }
    };

    EMBEDDINGS_GENERATED.fetch_add(1, Ordering::Relaxed);
    cache_put(text, emb.clone());
    emb
}

/// Cosine similarity between two embedding vectors. Returns [0.0, 1.0].
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let dot: f32 = a[..len].iter().zip(b[..len].iter()).map(|(x, y)| x * y).sum();
    dot.clamp(0.0, 1.0)
}

pub fn is_ollama_available()     -> bool { OLLAMA_EMBED_AVAILABLE.load(Ordering::Relaxed) }
pub fn embeddings_generated()    -> u64  { EMBEDDINGS_GENERATED.load(Ordering::Relaxed) }
pub fn embeddings_cached()       -> u64  { EMBEDDINGS_CACHED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tfidf_embed_produces_unit_vector() {
        let v = tfidf_embed("hello world test");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01 || norm < 0.001);
    }

    #[test]
    fn embed_no_panic() {
        let e = embed("the quick brown fox");
        assert_eq!(e.vector.len(), EMBED_DIM);
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 0.01);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine_similarity(&a, &b) < 0.01);
    }

    #[test]
    fn embed_cached_on_second_call() {
        let text = "cache_test_unique_phrase_xyz";
        embed(text);
        let before = EMBEDDINGS_CACHED.load(Ordering::Relaxed);
        embed(text);
        assert!(EMBEDDINGS_CACHED.load(Ordering::Relaxed) > before);
    }
}
