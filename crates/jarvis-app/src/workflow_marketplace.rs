//! Workflow marketplace — local workflow template library, import/export,
//! rating/tagging, and community workflow discovery (offline-only, no network).
//! All operations are local; no external services contacted.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static WORKFLOWS_PUBLISHED:  AtomicU64 = AtomicU64::new(0);
pub static WORKFLOWS_INSTALLED:  AtomicU64 = AtomicU64::new(0);
pub static WORKFLOWS_RATED:      AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum WorkflowCategory {
    Development,
    Research,
    Writing,
    DataAnalysis,
    Automation,
    Productivity,
    Voice,
    Custom,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowTemplate {
    pub id:          String,
    pub name:        String,
    pub description: String,
    pub author:      String,
    pub category:    WorkflowCategory,
    pub tags:        Vec<String>,
    pub steps:       Vec<WorkflowStep>,
    pub rating:      f32,
    pub rating_count: u32,
    pub installed:   bool,
    pub published_at: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowStep {
    pub order:       u8,
    pub name:        String,
    pub tool:        String,
    pub description: String,
    pub required:    bool,
}

struct MarketplaceState {
    templates: Vec<WorkflowTemplate>,
}

impl MarketplaceState {
    fn new() -> Self {
        Self {
            templates: vec![
                WorkflowTemplate {
                    id:          "code-review-flow".to_string(),
                    name:        "Code Review Assistant".to_string(),
                    description: "Systematic code review with memory context and inline suggestions".to_string(),
                    author:      "jarvis-team".to_string(),
                    category:    WorkflowCategory::Development,
                    tags:        vec!["code".to_string(), "review".to_string(), "quality".to_string()],
                    steps: vec![
                        WorkflowStep { order: 1, name: "Read file".to_string(),       tool: "file_read".to_string(),    description: "Read target source file".to_string(), required: true  },
                        WorkflowStep { order: 2, name: "Memory context".to_string(),  tool: "memory_search".to_string(), description: "Pull relevant past reviews".to_string(), required: false },
                        WorkflowStep { order: 3, name: "Analyze".to_string(),         tool: "reasoning".to_string(),    description: "Run deep reasoning pass".to_string(), required: true  },
                        WorkflowStep { order: 4, name: "Write feedback".to_string(),  tool: "file_write".to_string(),   description: "Output review notes".to_string(), required: true  },
                    ],
                    rating: 4.8, rating_count: 12, installed: true, published_at: 1_716_000_000,
                },
                WorkflowTemplate {
                    id:          "research-deep-dive".to_string(),
                    name:        "Deep Research Flow".to_string(),
                    description: "Multi-pass research with memory persistence and source tracking".to_string(),
                    author:      "jarvis-team".to_string(),
                    category:    WorkflowCategory::Research,
                    tags:        vec!["research".to_string(), "memory".to_string(), "analysis".to_string()],
                    steps: vec![
                        WorkflowStep { order: 1, name: "Define scope".to_string(),    tool: "reasoning".to_string(),    description: "Clarify research question".to_string(), required: true  },
                        WorkflowStep { order: 2, name: "Memory search".to_string(),   tool: "memory_search".to_string(), description: "Check existing knowledge".to_string(), required: true  },
                        WorkflowStep { order: 3, name: "Synthesize".to_string(),      tool: "reasoning".to_string(),    description: "Combine sources into insight".to_string(), required: true  },
                        WorkflowStep { order: 4, name: "Persist findings".to_string(), tool: "memory_write".to_string(), description: "Store to long-term memory".to_string(), required: true  },
                    ],
                    rating: 4.6, rating_count: 8, installed: true, published_at: 1_716_000_100,
                },
                WorkflowTemplate {
                    id:          "voice-task-capture".to_string(),
                    name:        "Voice Task Capture".to_string(),
                    description: "Capture spoken tasks, classify, and add to task queue".to_string(),
                    author:      "jarvis-team".to_string(),
                    category:    WorkflowCategory::Voice,
                    tags:        vec!["voice".to_string(), "tasks".to_string(), "productivity".to_string()],
                    steps: vec![
                        WorkflowStep { order: 1, name: "Listen".to_string(),          tool: "voice_stt".to_string(),    description: "Transcribe voice input".to_string(), required: true  },
                        WorkflowStep { order: 2, name: "Classify".to_string(),        tool: "reasoning".to_string(),    description: "Categorize task type".to_string(), required: true  },
                        WorkflowStep { order: 3, name: "Store task".to_string(),      tool: "memory_write".to_string(), description: "Add to task queue".to_string(), required: true  },
                        WorkflowStep { order: 4, name: "Confirm".to_string(),         tool: "voice_tts".to_string(),    description: "Speak confirmation".to_string(), required: false },
                    ],
                    rating: 4.5, rating_count: 20, installed: false, published_at: 1_716_000_200,
                },
            ],
        }
    }
}

static STATE: Lazy<Mutex<MarketplaceState>> = Lazy::new(|| Mutex::new(MarketplaceState::new()));

fn ts_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Browse / search ───────────────────────────────────────────────────────────

pub fn list_all() -> Vec<WorkflowTemplate> {
    STATE.lock().unwrap().templates.clone()
}

pub fn list_installed() -> Vec<WorkflowTemplate> {
    STATE.lock().unwrap().templates.iter()
        .filter(|t| t.installed)
        .cloned()
        .collect()
}

pub fn search(query: &str) -> Vec<WorkflowTemplate> {
    let q = query.to_lowercase();
    STATE.lock().unwrap().templates.iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.description.to_lowercase().contains(&q)
                || t.tags.iter().any(|tag| tag.contains(&q))
        })
        .cloned()
        .collect()
}

pub fn by_category(category: &WorkflowCategory) -> Vec<WorkflowTemplate> {
    STATE.lock().unwrap().templates.iter()
        .filter(|t| &t.category == category)
        .cloned()
        .collect()
}

pub fn get(id: &str) -> Option<WorkflowTemplate> {
    STATE.lock().unwrap().templates.iter().find(|t| t.id == id).cloned()
}

// ── Install / publish ─────────────────────────────────────────────────────────

pub fn install(id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    if let Some(t) = s.templates.iter_mut().find(|t| t.id == id) {
        if !t.installed {
            t.installed = true;
            WORKFLOWS_INSTALLED.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    }
    false
}

pub fn uninstall(id: &str) -> bool {
    let mut s = STATE.lock().unwrap();
    if let Some(t) = s.templates.iter_mut().find(|t| t.id == id) {
        t.installed = false;
        return true;
    }
    false
}

pub fn publish(template: WorkflowTemplate) -> bool {
    let mut s = STATE.lock().unwrap();
    if s.templates.iter().any(|t| t.id == template.id) { return false; }
    s.templates.push(template);
    WORKFLOWS_PUBLISHED.fetch_add(1, Ordering::Relaxed);
    true
}

// ── Rating ────────────────────────────────────────────────────────────────────

pub fn rate(id: &str, stars: u8) -> bool {
    let stars = stars.clamp(1, 5);
    let mut s = STATE.lock().unwrap();
    if let Some(t) = s.templates.iter_mut().find(|t| t.id == id) {
        let total = t.rating * t.rating_count as f32 + stars as f32;
        t.rating_count += 1;
        t.rating = total / t.rating_count as f32;
        WORKFLOWS_RATED.fetch_add(1, Ordering::Relaxed);
        return true;
    }
    false
}

// ── Export / import ───────────────────────────────────────────────────────────

pub fn export_template(id: &str) -> Option<String> {
    let s = STATE.lock().unwrap();
    s.templates.iter().find(|t| t.id == id)
        .map(|t| serde_json::to_string(t).unwrap_or_default())
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct MarketplaceSnapshot {
    pub workflows_published: u64,
    pub workflows_installed: u64,
    pub workflows_rated:     u64,
    pub total_templates:     usize,
    pub installed_count:     usize,
}

pub fn snapshot() -> MarketplaceSnapshot {
    let s = STATE.lock().unwrap();
    MarketplaceSnapshot {
        workflows_published: WORKFLOWS_PUBLISHED.load(Ordering::Relaxed),
        workflows_installed: WORKFLOWS_INSTALLED.load(Ordering::Relaxed),
        workflows_rated:     WORKFLOWS_RATED.load(Ordering::Relaxed),
        total_templates:     s.templates.len(),
        installed_count:     s.templates.iter().filter(|t| t.installed).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_all_nonempty() {
        assert!(!list_all().is_empty());
    }

    #[test]
    fn search_finds_code_review() {
        let results = search("code review");
        assert!(results.iter().any(|t| t.id == "code-review-flow"));
    }

    #[test]
    fn install_and_list_installed() {
        install("voice-task-capture");
        assert!(list_installed().iter().any(|t| t.id == "voice-task-capture"));
    }

    #[test]
    fn rate_workflow() {
        let before = WORKFLOWS_RATED.load(Ordering::Relaxed);
        rate("research-deep-dive", 5);
        assert!(WORKFLOWS_RATED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn publish_new_workflow() {
        let t = WorkflowTemplate {
            id: "custom-test-wf".to_string(),
            name: "Custom Test".to_string(),
            description: "Test workflow".to_string(),
            author: "tester".to_string(),
            category: WorkflowCategory::Custom,
            tags: vec!["test".to_string()],
            steps: vec![],
            rating: 0.0,
            rating_count: 0,
            installed: false,
            published_at: ts_now(),
        };
        let before = WORKFLOWS_PUBLISHED.load(Ordering::Relaxed);
        assert!(publish(t));
        assert!(WORKFLOWS_PUBLISHED.load(Ordering::Relaxed) > before);
    }

    #[test]
    fn export_template_is_json() {
        let json = export_template("code-review-flow");
        assert!(json.is_some());
        assert!(json.unwrap().contains("code-review-flow"));
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert!(s.total_templates > 0);
    }
}
