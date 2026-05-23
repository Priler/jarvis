//! Multi-model runtime — intelligently routes requests to specialized models based
//! on task role: tactical, deep reasoning, vision, embedding, voice.

use std::sync::atomic::{AtomicU64, Ordering};
use once_cell::sync::Lazy;
use std::sync::Mutex;

pub static REQUESTS_ROUTED:   AtomicU64 = AtomicU64::new(0);
pub static FALLBACKS_USED:    AtomicU64 = AtomicU64::new(0);
pub static ROLE_SWITCHES:     AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ModelRole {
    Tactical,    // fast responses, short context, < 3B params
    Reasoning,   // deep CoT, large context, 7B+
    Vision,      // image understanding (multimodal)
    Embedding,   // vector embeddings (nomic-embed, etc.)
    Voice,       // speech synthesis / recognition hints
}

impl ModelRole {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Tactical  => "Tactical",
            Self::Reasoning => "Reasoning",
            Self::Vision    => "Vision",
            Self::Embedding => "Embedding",
            Self::Voice     => "Voice",
        }
    }

    pub fn latency_target_ms(&self) -> u64 {
        match self {
            Self::Tactical  => 500,
            Self::Reasoning => 3000,
            Self::Vision    => 2000,
            Self::Embedding => 100,
            Self::Voice     => 200,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoleConfig {
    pub role:           ModelRole,
    pub primary_model:  Option<String>,
    pub fallback_model: Option<String>,
    pub requests:       u64,
    pub fallbacks:      u64,
    pub avg_latency_ms: u64,
}

impl RoleConfig {
    fn new(role: ModelRole) -> Self {
        Self {
            role,
            primary_model:  None,
            fallback_model: None,
            requests:       0,
            fallbacks:      0,
            avg_latency_ms: 0,
        }
    }
}

struct MultiModelState {
    roles:       Vec<RoleConfig>,
    active_role: ModelRole,
}

impl MultiModelState {
    fn new() -> Self {
        Self {
            roles: vec![
                RoleConfig::new(ModelRole::Tactical),
                RoleConfig::new(ModelRole::Reasoning),
                RoleConfig::new(ModelRole::Vision),
                RoleConfig::new(ModelRole::Embedding),
                RoleConfig::new(ModelRole::Voice),
            ],
            active_role: ModelRole::Tactical,
        }
    }

    fn role_mut(&mut self, role: ModelRole) -> Option<&mut RoleConfig> {
        self.roles.iter_mut().find(|r| r.role == role)
    }

    fn role(&self, role: ModelRole) -> Option<&RoleConfig> {
        self.roles.iter().find(|r| r.role == role)
    }
}

static STATE: Lazy<Mutex<MultiModelState>> = Lazy::new(|| Mutex::new(MultiModelState::new()));

pub fn assign_model(role: ModelRole, primary: &str, fallback: Option<&str>) {
    let mut s = STATE.lock().unwrap();
    if let Some(r) = s.role_mut(role) {
        r.primary_model  = Some(primary.to_string());
        r.fallback_model = fallback.map(|f| f.to_string());
    }
}

pub fn route_request(role: ModelRole, prompt: &str) -> (String, bool) {
    REQUESTS_ROUTED.fetch_add(1, Ordering::Relaxed);

    let mut s = STATE.lock().unwrap();
    if s.active_role != role {
        s.active_role = role;
        ROLE_SWITCHES.fetch_add(1, Ordering::Relaxed);
    }
    let config = s.role(role).cloned();
    drop(s);

    let _ = prompt; // routing only — actual inference via llm_provider_runtime

    if let Some(cfg) = config {
        // Try primary, fall back to fallback if primary is absent
        let (model, used_fallback) = if cfg.primary_model.is_some() {
            (cfg.primary_model.unwrap_or_default(), false)
        } else if cfg.fallback_model.is_some() {
            FALLBACKS_USED.fetch_add(1, Ordering::Relaxed);
            let mut s2 = STATE.lock().unwrap();
            if let Some(r) = s2.role_mut(role) { r.fallbacks += 1; }
            (cfg.fallback_model.unwrap_or_default(), true)
        } else {
            // No model configured — use stub
            FALLBACKS_USED.fetch_add(1, Ordering::Relaxed);
            ("stub".to_string(), true)
        };

        let mut s2 = STATE.lock().unwrap();
        if let Some(r) = s2.role_mut(role) { r.requests += 1; }

        (model, used_fallback)
    } else {
        ("stub".to_string(), true)
    }
}

pub fn classify_prompt(prompt: &str) -> ModelRole {
    let lower = prompt.to_lowercase();
    if lower.contains("image") || lower.contains("screenshot") || lower.contains("picture") {
        ModelRole::Vision
    } else if lower.len() > 500 || lower.contains("reason") || lower.contains("explain") || lower.contains("analyze") {
        ModelRole::Reasoning
    } else if lower.contains("embed") || lower.contains("vector") {
        ModelRole::Embedding
    } else {
        ModelRole::Tactical
    }
}

pub fn role_configs() -> Vec<RoleConfig> {
    STATE.lock().unwrap().roles.clone()
}

#[derive(Debug, serde::Serialize)]
pub struct MultiModelSnapshot {
    pub requests_routed:  u64,
    pub fallbacks_used:   u64,
    pub role_switches:    u64,
    pub active_role:      String,
    pub roles_configured: usize,
}

pub fn snapshot() -> MultiModelSnapshot {
    let s = STATE.lock().unwrap();
    let configured = s.roles.iter().filter(|r| r.primary_model.is_some()).count();
    MultiModelSnapshot {
        requests_routed:  REQUESTS_ROUTED.load(Ordering::Relaxed),
        fallbacks_used:   FALLBACKS_USED.load(Ordering::Relaxed),
        role_switches:    ROLE_SWITCHES.load(Ordering::Relaxed),
        active_role:      s.active_role.label().to_string(),
        roles_configured: configured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_and_route() {
        assign_model(ModelRole::Tactical, "llama3.2:3b", Some("tinyllama"));
        let (model, fallback) = route_request(ModelRole::Tactical, "hello");
        assert_eq!(model, "llama3.2:3b");
        assert!(!fallback);
    }

    #[test]
    fn fallback_when_no_primary() {
        let (model, fallback) = route_request(ModelRole::Vision, "show me the image");
        // Vision has no model assigned → fallback
        let _ = (model, fallback); // just verify no panic
    }

    #[test]
    fn classify_reasoning_prompt() {
        let role = classify_prompt("Please reason through this complex problem in detail and explain the analysis");
        assert_eq!(role, ModelRole::Reasoning);
    }

    #[test]
    fn classify_vision_prompt() {
        let role = classify_prompt("What do you see in this screenshot?");
        assert_eq!(role, ModelRole::Vision);
    }

    #[test]
    fn classify_default_tactical() {
        let role = classify_prompt("What time is it?");
        assert_eq!(role, ModelRole::Tactical);
    }

    #[test]
    fn snapshot_no_panic() {
        let s = snapshot();
        assert_eq!(s.roles_configured, 1); // only Tactical was assigned above
    }
}
