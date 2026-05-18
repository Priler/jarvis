import { writable } from "svelte/store"
import type { JarvisState, ExecutedCommand, PendingConfirmation } from "./types"

export const jarvisState          = writable<JarvisState>("disconnected")
export const ipcConnected         = writable(false)
export const lastRecognizedText   = writable("")
export const lastExecutedCommand  = writable<ExecutedCommand | null>(null)
export const lastError            = writable("")
export const pendingConfirmation  = writable<PendingConfirmation | null>(null)
export const sandboxWarnings      = writable<string[]>([])
