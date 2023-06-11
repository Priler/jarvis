use tauri::Manager;

// the payload type must implement `Serialize` and `Clone`.
#[derive(Clone, serde::Serialize)]
pub struct Payload {
    pub data: String,
}

#[allow(dead_code)]
pub enum EventTypes {
    AudioPlay,
    AssistantWaiting,
    AssistantGreet,
    CommandStart,
    CommandInProcess,
    CommandEnd,
}

impl EventTypes {
    pub fn get(&self) -> &str {
        match self {
            Self::AudioPlay => "audio-play",
            Self::AssistantWaiting => "assistant-waiting",
            Self::AssistantGreet => "assistant-greet",
            Self::CommandStart => "command-start",
            Self::CommandInProcess => "command-in-process",
            Self::CommandEnd => "command-end",
        }
    }
}

pub fn play(phrase: &str, app_handle: &tauri::AppHandle) {
    app_handle
        .emit_all(
            EventTypes::AudioPlay.get(),
            Payload {
                data: phrase.into(),
            },
        )
        .unwrap();
}
