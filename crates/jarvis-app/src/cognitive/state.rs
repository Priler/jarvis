#![allow(dead_code)]

#[derive(Debug, Clone, PartialEq)]
pub enum CognitiveState {
    Idle,
    Listening,
    Understanding,
    Reasoning,
    Planning,
    Executing { step: usize, total: usize },
    Observing,
    Recovering,
    Conversing,
    AwaitingClarification,
}

impl CognitiveState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CognitiveState::Idle => "idle",
            CognitiveState::Listening => "listening",
            CognitiveState::Understanding => "understanding",
            CognitiveState::Reasoning => "reasoning",
            CognitiveState::Planning => "planning",
            CognitiveState::Executing { .. } => "executing",
            CognitiveState::Observing => "observing",
            CognitiveState::Recovering => "recovering",
            CognitiveState::Conversing => "conversing",
            CognitiveState::AwaitingClarification => "awaiting_clarification",
        }
    }
}
