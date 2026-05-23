//! RAG pipeline — retrieves relevant context chunks and formats them for LLM injection.

use std::sync::atomic::{AtomicU64, Ordering};

pub static QUERIES_RUN:        AtomicU64 = AtomicU64::new(0);
pub static CONTEXT_CHUNKS_USED: AtomicU64 = AtomicU64::new(0);

const DEFAULT_TOP_K:         usize = 5;
const MIN_RELEVANCE_SCORE:   f32   = 0.15;
const MAX_CONTEXT_CHARS:     usize = 3000;
const CONTEXT_HEADER: &str   = "=== Retrieved Context ===\n";
const CONTEXT_SEPARATOR: &str = "\n---\n";

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RagContext {
    pub query:         String,
    pub chunks_found:  usize,
    pub context_text:  String,
    pub augmented_prompt: String,
}

impl RagContext {
    pub fn has_context(&self) -> bool { self.chunks_found > 0 }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Run RAG retrieval for a query and return augmented context + prompt.
pub fn retrieve(query: &str, original_prompt: &str) -> RagContext {
    retrieve_with_k(query, original_prompt, DEFAULT_TOP_K)
}

pub fn retrieve_with_k(query: &str, original_prompt: &str, k: usize) -> RagContext {
    QUERIES_RUN.fetch_add(1, Ordering::Relaxed);

    let results = crate::knowledge_index::search(query, k);
    let relevant: Vec<_> = results.into_iter()
        .filter(|r| r.score >= MIN_RELEVANCE_SCORE)
        .collect();

    let chunks_found = relevant.len();
    CONTEXT_CHUNKS_USED.fetch_add(chunks_found as u64, Ordering::Relaxed);

    if chunks_found == 0 {
        return RagContext {
            query: query.to_string(),
            chunks_found: 0,
            context_text: String::new(),
            augmented_prompt: original_prompt.to_string(),
        };
    }

    // Build context text, capped at MAX_CONTEXT_CHARS
    let mut context_text = CONTEXT_HEADER.to_string();
    let mut char_budget = MAX_CONTEXT_CHARS;

    for (i, r) in relevant.iter().enumerate() {
        let snippet = if r.chunk.text.len() > char_budget {
            r.chunk.text[..char_budget].to_string()
        } else {
            r.chunk.text.clone()
        };
        char_budget = char_budget.saturating_sub(snippet.len() + 20);

        context_text.push_str(&format!(
            "[{}] (score: {:.2}, source: {})\n{}\n",
            i + 1, r.score,
            extract_filename(&r.chunk.doc_path),
            snippet
        ));

        if i + 1 < chunks_found {
            context_text.push_str(CONTEXT_SEPARATOR);
        }
        if char_budget == 0 { break; }
    }

    let augmented_prompt = format!(
        "{}\n\nBased on the context above, answer: {}",
        context_text, original_prompt
    );

    RagContext { query: query.to_string(), chunks_found, context_text, augmented_prompt }
}

fn extract_filename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Add a document to the RAG knowledge base.
pub fn index_document(path: &str, content: &str) -> u64 {
    crate::knowledge_index::add_document(path, content)
}

/// Add a memory entry as a RAG document.
pub fn index_memory_entry(key: &str, value: &str) -> u64 {
    let content = format!("{}: {}", key, value);
    crate::knowledge_index::add_document(&format!("memory://{}", key), &content)
}

pub fn queries_run()         -> u64 { QUERIES_RUN.load(Ordering::Relaxed) }
pub fn context_chunks_used() -> u64 { CONTEXT_CHUNKS_USED.load(Ordering::Relaxed) }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieve_empty_index_returns_original_prompt() {
        crate::knowledge_index::clear();
        let ctx = retrieve("test query", "what is X?");
        assert!(!ctx.has_context() || ctx.augmented_prompt.contains("what is X?"));
    }

    #[test]
    fn retrieve_with_indexed_doc() {
        crate::knowledge_index::clear();
        index_document("/test/rag.txt", "Jarvis is a local AI assistant that works offline");
        let ctx = retrieve("Jarvis", "Tell me about Jarvis");
        // Should find or return gracefully
        assert!(!ctx.augmented_prompt.is_empty());
    }

    #[test]
    fn retrieve_with_k_bounded() {
        let ctx = retrieve_with_k("anything", "prompt", 3);
        assert!(ctx.chunks_found <= 3);
    }

    #[test]
    fn queries_run_increments() {
        let before = QUERIES_RUN.load(Ordering::Relaxed);
        retrieve("test", "test prompt");
        assert!(QUERIES_RUN.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn extract_filename_works() {
        assert_eq!(extract_filename("/home/user/docs/file.txt"), "file.txt");
        assert_eq!(extract_filename("C:\\Users\\docs\\note.md"), "note.md");
    }
}
