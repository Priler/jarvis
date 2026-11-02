use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    #[serde(skip)]
    pub path: PathBuf,

    pub voice: VoiceMeta,

    // Multi-language reactions
    pub reactions: HashMap<String, VoiceReactions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceMeta {
    pub id: String,
    pub name: String,

    #[serde(default)]
    pub author: String,

    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceReactions {
    // app startup (time-based or generic)
    #[serde(default)]
    pub greet: Vec<String>,

    #[serde(default)]
    pub greet_morning: Vec<String>,
    #[serde(default)]
    pub greet_day: Vec<String>,
    #[serde(default)]
    pub greet_evening: Vec<String>,
    #[serde(default)]
    pub greet_night: Vec<String>,

    // wake word detected
    #[serde(default)]
    pub reply: Vec<String>,

    // command executed
    #[serde(default)]
    pub ok: Vec<String>,

    // command not found
    #[serde(default)]
    pub not_found: Vec<String>,

    // thank you
    #[serde(default)]
    pub thanks: Vec<String>,

    // error
    #[serde(default)]
    pub error: Vec<String>,

    // shutdown
    #[serde(default)]
    pub goodbye: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum Reaction {
    Greet, // app startup
    Reply, // wake word detected
    Ok,    // command executed
    NotFound,
    Thanks,
    Error,
    Goodbye,
}
