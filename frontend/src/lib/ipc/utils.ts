import type { IpcMessage } from "./types"

/**
 * Parse a raw WebSocket message string into a typed IpcMessage.
 * Returns null if the payload is not valid JSON or lacks an event field.
 */
export function parseIpcMessage(raw: string): IpcMessage | null {
    try {
        const data: unknown = JSON.parse(raw)
        if (data !== null && typeof data === "object" && "event" in data && typeof (data as Record<string, unknown>).event === "string") {
            return data as IpcMessage
        }
        return null
    } catch {
        return null
    }
}

/**
 * Exponential backoff delay capped at maxMs.
 * Pure function — no side effects, fully testable.
 */
export function computeReconnectDelay(
    attempt:  number,
    baseMs:   number = 1000,
    maxMs:    number = 3000,
): number {
    return Math.min(baseMs * Math.pow(2, attempt), maxMs)
}
