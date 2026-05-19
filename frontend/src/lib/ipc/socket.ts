import { get } from "svelte/store"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { invoke } from "@tauri-apps/api/core"
import { jarvisState, ipcConnected, lastRecognizedText, lastExecutedCommand, lastError, pendingConfirmation, sandboxWarnings, loadingComponent } from "./stores"
import type { IpcMessage, IpcOutgoing } from "./types"
import { parseIpcMessage, computeReconnectDelay } from "./utils"
import { addToast } from "../toast"

// ### CONNECTION ###

const DEV = import.meta.env.DEV

const IPC_PORT              = 9712
const RECONNECT_BASE_MS     = 1000
const RECONNECT_MAX_MS      = 3000
const HEARTBEAT_INTERVAL_MS = 30000
const HEARTBEAT_TIMEOUT_MS  = 5000
const PENDING_COMMANDS_MAX  = 20

let ws: WebSocket | null = null
let _ipcToken: string | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let heartbeatTimer: ReturnType<typeof setInterval> | null = null
let heartbeatTimeoutTimer: ReturnType<typeof setTimeout> | null = null
let processingTimeoutTimer: ReturnType<typeof setTimeout> | null = null
let reconnectAttempt = 0
let manualDisconnect = false
let enabled = false
let pendingTextCommands: string[] = []
let _cmdSeq = 0
let _errorToastShown = false
let _watchdogFired = false

// Number of failed reconnect attempts before trying to restart jarvis-app.
const WATCHDOG_TRIGGER_ATTEMPT = 3

/** Store the IPC auth token fetched from jarvis-gui at startup. */
export function setIpcToken(token: string | null) {
    _ipcToken = token
}

export function enableIpc() {
    enabled = true
    reconnectAttempt = 0
    if (reconnectTimer) {
        clearTimeout(reconnectTimer)
        reconnectTimer = null
    }
    connectIpc()
}

export function disableIpc() {
    enabled = false
    disconnectIpc()
}

export function connectIpc(port: number = IPC_PORT) {
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return
    manualDisconnect = false

    ws = new WebSocket(`ws://127.0.0.1:${port}`)

    ws.onopen = () => {
        // Send auth token as very first message if configured
        if (_ipcToken) {
            ws?.send(JSON.stringify({ action: "auth", token: _ipcToken } satisfies IpcOutgoing))
        }
        ipcConnected.set(true)
        jarvisState.set("idle")
        lastError.set("")
        reconnectAttempt = 0
        _errorToastShown = false
        _watchdogFired = false
        startHeartbeat()
        flushPendingCommands()
        DEV && console.log("[IPC] connected")
    }

    ws.onclose = () => {
        ipcConnected.set(false)
        jarvisState.set("disconnected")
        stopHeartbeat()
        scheduleReconnect()
        DEV && console.log("[IPC] disconnected")
    }

    ws.onerror = (err) => {
        DEV && console.error("[IPC] error:", err)
        if (!_errorToastShown) {
            _errorToastShown = true
            addToast("Lost connection to jarvis-app", "error")
        }
    }

    ws.onmessage = (event) => {
        const msg = parseIpcMessage(event.data)
        if (msg) {
            handleEvent(msg)
        } else {
            DEV && console.error("[IPC] failed to parse message:", event.data)
        }
    }
}

function scheduleReconnect() {
    if (reconnectTimer || manualDisconnect || !enabled) return

    const delay = computeReconnectDelay(reconnectAttempt, RECONNECT_BASE_MS, RECONNECT_MAX_MS)
    reconnectAttempt++
    DEV && console.log(`[IPC] Reconnecting in ${delay / 1000}s (attempt ${reconnectAttempt})...`)

    // Watchdog: after N failures try to restart jarvis-app (once per disconnection session).
    if (reconnectAttempt === WATCHDOG_TRIGGER_ATTEMPT && !_watchdogFired) {
        _watchdogFired = true
        invoke("run_jarvis_app").catch((e) => {
            DEV && console.warn("[IPC] Watchdog restart failed:", e)
        })
    }

    reconnectTimer = setTimeout(() => {
        reconnectTimer = null
        connectIpc()
    }, delay)
}

export function disconnectIpc() {
    manualDisconnect = true
    reconnectAttempt = 0
    pendingTextCommands = []
    stopHeartbeat()
    clearProcessingTimeout()

    if (reconnectTimer) {
        clearTimeout(reconnectTimer)
        reconnectTimer = null
    }

    if (ws) {
        ws.close()
        ws = null
    }

    ipcConnected.set(false)
    jarvisState.set("disconnected")
}

function startHeartbeat() {
    stopHeartbeat()
    heartbeatTimer = setInterval(() => {
        if (ws?.readyState !== WebSocket.OPEN) return
        // Skip ping while backend is actively processing to avoid false reconnects
        if (get(jarvisState) === "processing") return
        ws.send(JSON.stringify({ action: "ping" }))
        heartbeatTimeoutTimer = setTimeout(() => {
            DEV && console.warn("[IPC] heartbeat timeout — forcing reconnect")
            ws?.close()
        }, HEARTBEAT_TIMEOUT_MS)
    }, HEARTBEAT_INTERVAL_MS)
}

function stopHeartbeat() {
    if (heartbeatTimer) {
        clearInterval(heartbeatTimer)
        heartbeatTimer = null
    }
    if (heartbeatTimeoutTimer) {
        clearTimeout(heartbeatTimeoutTimer)
        heartbeatTimeoutTimer = null
    }
}

const PROCESSING_TIMEOUT_MS = 60_000

function startProcessingTimeout() {
    clearProcessingTimeout()
    processingTimeoutTimer = setTimeout(() => {
        DEV && console.warn("[IPC] processing timeout — forcing idle after 60s")
        processingTimeoutTimer = null
        jarvisState.set("idle")
    }, PROCESSING_TIMEOUT_MS)
}

function clearProcessingTimeout() {
    if (processingTimeoutTimer) {
        clearTimeout(processingTimeoutTimer)
        processingTimeoutTimer = null
    }
}

// ### EVENT HANDLING ###

function handleEvent(data: IpcMessage) {
    DEV && console.log("IPC: Event", data.event, data)

    switch (data.event) {
        case "wake_word_detected":
        case "listening":
            clearProcessingTimeout()
            jarvisState.set("listening")
            break

        case "speech_recognized":
            lastRecognizedText.set(data.text)
            jarvisState.set("processing")
            startProcessingTimeout()
            break

        case "command_executed":
            clearProcessingTimeout()
            lastExecutedCommand.set({ id: data.id, seq: ++_cmdSeq })
            break

        case "idle":
            clearProcessingTimeout()
            jarvisState.set("idle")
            loadingComponent.set(null)
            break

        case "error":
            clearProcessingTimeout()
            lastError.set(data.message || "Unknown error")
            break

        case "started":
            jarvisState.set("idle")
            break

        case "stopping":
            clearProcessingTimeout()
            jarvisState.set("disconnected")
            break

        case "pong":
            if (heartbeatTimeoutTimer) {
                clearTimeout(heartbeatTimeoutTimer)
                heartbeatTimeoutTimer = null
            }
            break

        case "reveal_window":
            revealWindow()
            break

        case "confirmation_required":
            pendingConfirmation.set({ id: data.id, description: data.description, cmd: data.cmd })
            break

        case "sandbox_warning":
            sandboxWarnings.set(data.commands)
            DEV && console.warn("[IPC] Commands with full sandbox access:", data.commands)
            break

        case "loading":
            loadingComponent.set(data.component || null)
            break
    }
}

// ### ACTIONS ###

function sendAction(msg: IpcOutgoing): boolean {
    if (ws?.readyState !== WebSocket.OPEN) return false
    ws.send(JSON.stringify(msg))
    return true
}

export function stopJarvisApp() {
    return sendAction({ action: "stop" })
}

export function reloadCommands() {
    return sendAction({ action: "reload_commands" })
}

export function sendConfirmResult(id: string, approved: boolean): boolean {
    return sendAction({ action: "confirm_result", id, approved })
}

export function sendTextCommand(text: string): boolean {
    if (sendAction({ action: "text_command", text })) return true
    if (pendingTextCommands.length >= PENDING_COMMANDS_MAX) {
        pendingTextCommands.shift()
    }
    pendingTextCommands.push(text)
    return false
}

function flushPendingCommands() {
    const commands = [...pendingTextCommands]
    pendingTextCommands = []
    commands.forEach((text, i) => {
        setTimeout(() => {
            if (ws?.readyState === WebSocket.OPEN) {
                sendAction({ action: "text_command", text })
            }
        }, i * 100)
    })
}

async function revealWindow() {
    try {
        const window = getCurrentWindow()
        await window.show()
        await window.unminimize()
        await window.setFocus()
    } catch (err: unknown) {
        DEV && console.error("[IPC] Failed to reveal window:", err)
    }
}
