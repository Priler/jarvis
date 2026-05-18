import { writable } from "svelte/store"
import type { JarvisState, ExecutedCommand, PendingConfirmation } from "./types"

export const jarvisState          = writable<JarvisState>("disconnected")
export const ipcConnected         = writable(false)
export const lastRecognizedText   = writable("")
export const lastExecutedCommand  = writable<ExecutedCommand | null>(null)
export const lastError            = writable("")
export const pendingConfirmation  = writable<PendingConfirmation | null>(null)
export const sandboxWarnings      = writable<string[]>([])
// Non-null while jarvis-app is in a slow init step (e.g. "stt", "audio", "intent").
export const loadingComponent     = writable<string | null>(null)
