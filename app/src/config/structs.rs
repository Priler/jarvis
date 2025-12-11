use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum WakeWordEngine {
    Rustpotter,
    Vosk,
    Porcupine,
}

impl fmt::Display for WakeWordEngine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SpeechToTextEngine {
    Vosk,
}

impl fmt::Display for SpeechToTextEngine {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
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

// pub enum TextToSpeechEngine {}

// pub enum IntentRecognitionEngine {}
