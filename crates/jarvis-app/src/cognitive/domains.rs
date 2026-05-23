use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Domain {
    Media,
    Browser,
    System,
    Productivity,
    Communication,
    Automation,
    Knowledge,
    Memory,
    #[default]
    Unknown,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Domain::Media => "media",
            Domain::Browser => "browser",
            Domain::System => "system",
            Domain::Productivity => "productivity",
            Domain::Communication => "communication",
            Domain::Automation => "automation",
            Domain::Knowledge => "knowledge",
            Domain::Memory => "memory",
            Domain::Unknown => "unknown",
        }
    }
}

pub fn classify_domain(text: &str) -> Domain {
    let t = text.to_lowercase();

    if matches_any(&t, &[
        "music", "play", "pause", "stop", "next", "previous", "volume",
        "song", "track", "playlist", "youtube", "spotify", "video", "movie",
        "film", "audio", "mute", "unmute",
    ]) {
        return Domain::Media;
    }
    if matches_any(&t, &[
        "browser", "website", "google", "search online", "url", "http",
        "navigate", "tab", "chrome", "firefox", "edge", "safari",
    ]) {
        return Domain::Browser;
    }
    if matches_any(&t, &[
        "shutdown", "restart", "sleep", "lock", "screenshot", "settings",
        "task manager", "process", "cpu", "disk", "wifi", "bluetooth",
        "display", "brightness", "update",
    ]) {
        return Domain::System;
    }
    if matches_any(&t, &[
        "note", "remind", "todo", "calendar", "meeting", "schedule",
        "alarm", "timer", "stopwatch", "write", "document", "folder",
    ]) {
        return Domain::Productivity;
    }
    if matches_any(&t, &[
        "call", "message", "email", "send", "contact", "chat",
        "whatsapp", "telegram", "discord", "slack",
    ]) {
        return Domain::Communication;
    }
    if matches_any(&t, &[
        "run", "execute", "script", "automate", "macro", "shortcut",
        "hotkey", "launch", "install", "uninstall",
    ]) {
        return Domain::Automation;
    }
    if matches_any(&t, &[
        "what is", "who is", "where is", "when is", "how to",
        "explain", "define", "weather", "news", "tell me",
    ]) {
        return Domain::Knowledge;
    }
    if matches_any(&t, &[
        "remember", "forget", "recall", "history", "what did i",
        "last time", "context",
    ]) {
        return Domain::Memory;
    }

    Domain::Unknown
}

fn matches_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}
