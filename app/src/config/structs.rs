use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum WakeWordEngine {
    Rustpotter,
    Vosk,
    Porcupine
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SpeechToTextEngine {
    Vosk
}

#[derive(PartialEq, Debug)]
pub enum RecorderType {
    Cpal,
    PvRecorder,
    PortAudio
}

#[derive(PartialEq, Debug)]
pub enum AudioType {
    Rodio,
    Kira
}

// pub enum TextToSpeechEngine {}

// pub enum IntentRecognitionEngine {}