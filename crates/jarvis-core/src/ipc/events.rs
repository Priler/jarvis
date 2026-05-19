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

#[cfg(test)]
mod tests {
    use super::*;

    fn to_val(event: &IpcEvent) -> serde_json::Value {
        serde_json::to_value(event).unwrap()
    }

    fn from_str(s: &str) -> IpcAction {
        serde_json::from_str(s).unwrap()
    }

    // --- IpcEvent serialization ---

    #[test]
    fn event_idle_serializes_tag() {
        let val = to_val(&IpcEvent::Idle);
        assert_eq!(val["event"], "idle");
    }

    #[test]
    fn event_speech_recognized_includes_text() {
        let val = to_val(&IpcEvent::SpeechRecognized { text: "jarvis".to_string() });
        assert_eq!(val["event"], "speech_recognized");
        assert_eq!(val["text"], "jarvis");
    }

    #[test]
    fn event_error_includes_message() {
        let val = to_val(&IpcEvent::Error { message: "oops".to_string() });
        assert_eq!(val["event"], "error");
        assert_eq!(val["message"], "oops");
    }

    #[test]
    fn event_command_executed_includes_id_and_success() {
        let val = to_val(&IpcEvent::CommandExecuted { id: "cmd-1".to_string(), success: true });
        assert_eq!(val["event"], "command_executed");
        assert_eq!(val["id"], "cmd-1");
        assert_eq!(val["success"], true);
    }

    #[test]
    fn event_loading_includes_component() {
        let val = to_val(&IpcEvent::Loading { component: "stt".to_string() });
        assert_eq!(val["event"], "loading");
        assert_eq!(val["component"], "stt");
    }

    #[test]
    fn event_confirmation_required_includes_all_fields() {
        let val = to_val(&IpcEvent::ConfirmationRequired {
            id: "confirm-1".to_string(),
            description: "Are you sure?".to_string(),
            cmd: "shutdown".to_string(),
        });
        assert_eq!(val["event"], "confirmation_required");
        assert_eq!(val["id"], "confirm-1");
        assert_eq!(val["description"], "Are you sure?");
        assert_eq!(val["cmd"], "shutdown");
    }

    #[test]
    fn event_sandbox_warning_includes_commands_list() {
        let val = to_val(&IpcEvent::SandboxWarning {
            commands: vec!["cmd-a".to_string(), "cmd-b".to_string()],
        });
        assert_eq!(val["event"], "sandbox_warning");
        assert_eq!(val["commands"][0], "cmd-a");
        assert_eq!(val["commands"][1], "cmd-b");
    }

    // --- IpcAction deserialization ---

    #[test]
    fn action_ping_deserializes() {
        let action = from_str(r#"{"action":"ping"}"#);
        assert!(matches!(action, IpcAction::Ping));
    }

    #[test]
    fn action_stop_deserializes() {
        let action = from_str(r#"{"action":"stop"}"#);
        assert!(matches!(action, IpcAction::Stop));
    }

    #[test]
    fn action_auth_deserializes_token() {
        let action = from_str(r#"{"action":"auth","token":"abc123"}"#);
        assert!(matches!(action, IpcAction::Auth { token } if token == "abc123"));
    }

    #[test]
    fn action_set_muted_deserializes() {
        let action = from_str(r#"{"action":"set_muted","muted":true}"#);
        assert!(matches!(action, IpcAction::SetMuted { muted: true }));
    }

    #[test]
    fn action_text_command_deserializes() {
        let action = from_str(r#"{"action":"text_command","text":"open browser"}"#);
        assert!(matches!(action, IpcAction::TextCommand { text } if text == "open browser"));
    }

    #[test]
    fn action_confirm_result_deserializes_approved() {
        let action = from_str(r#"{"action":"confirm_result","id":"x","approved":false}"#);
        assert!(matches!(action, IpcAction::ConfirmResult { id, approved: false } if id == "x"));
    }

    #[test]
    fn action_unknown_tag_is_error() {
        let result: Result<IpcAction, _> = serde_json::from_str(r#"{"action":"not_a_thing"}"#);
        assert!(result.is_err());
    }
}