use serde::{Deserialize, Serialize};

// Events sent from jarvis-app to GUI
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum IpcEvent {
    // Wake word detected, starting to listen
    WakeWordDetected,

    // Actively listening for command
    Listening,

    // Speech recognized
    SpeechRecognized { text: String },

    // Command was executed
    CommandExecuted { id: String, success: bool },

    // Returned to idle state
    Idle,

    // Error occurred
    Error { message: String },

    // App started
    Started,

    // App is shutting down
    Stopping,

    // Pong response
    Pong,

    // request GUI to reveal/focus window
    RevealWindow,
}

// Actions sent from GUI to jarvis-app
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum IpcAction {
    // Request graceful shutdown
    Stop,

    // Reload commands from disk
    ReloadCommands,

    // Ping to check connection
    Ping,

    // Mute/unmute listening
    SetMuted { muted: bool },

    // Execute text command
    TextCommand { text: String },
}
