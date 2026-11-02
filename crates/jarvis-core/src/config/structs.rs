use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum WakeWordEngine {
    Rustpotter,
    Vosk,
    Porcupine,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum NoiseSuppressionBackend {
    None,
    Nnnoiseless,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SpeechToTextEngine {
    Vosk,
}

#[derive(PartialEq, Debug)]
pub enum RecorderType {
    Cpal,
    PvRecorder,
    PortAudio,
}

#[derive(PartialEq, Debug)]
pub enum AudioType {
    Rodio,
    Kira,
}

impl fmt::Display for WakeWordEngine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for SpeechToTextEngine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for NoiseSuppressionBackend {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
