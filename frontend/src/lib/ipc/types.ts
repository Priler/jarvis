export type JarvisState = "disconnected" | "idle" | "listening" | "processing"

/** Identifies a command execution event. seq increments on every event to force
 *  Svelte store subscribers to fire even when the same command runs twice in a row. */
export interface ExecutedCommand {
    id:  string
    seq: number
}

/** Pending CLI confirmation request from jarvis-app. */
export interface PendingConfirmation {
    id: string
    description: string
    cmd: string
}

export type IpcMessage =
    | { event: "wake_word_detected" }
    | { event: "listening" }
    | { event: "speech_recognized"; text: string }
    | { event: "command_executed"; id: string }
    | { event: "idle" }
    | { event: "error"; message: string }
    | { event: "started" }
    | { event: "stopping" }
    | { event: "pong" }
    | { event: "reveal_window" }
    | { event: "confirmation_required"; id: string; description: string; cmd: string }
    | { event: "sandbox_warning"; commands: string[] }
    | { event: "loading"; component: string }

export type IpcOutgoing =
    | { action: "stop" }
    | { action: "reload_commands" }
    | { action: "text_command"; text: string }
    | { action: "auth"; token: string }
    | { action: "confirm_result"; id: string; approved: boolean }
