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

    // CLI command requires user confirmation before executing
    ConfirmationRequired { id: String, description: String, cmd: String },

    // One or more Lua commands have sandbox = "full" (arbitrary shell access)
    SandboxWarning { commands: Vec<String> },

    // A slow initialization step is in progress (e.g. loading STT model)
    Loading { component: String },
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

    // Authenticate with IPC server token
    Auth { token: String },

    // Confirm or deny a pending CLI command execution
    ConfirmResult { id: String, approved: bool },
}