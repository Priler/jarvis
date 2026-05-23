import { writable } from "svelte/store"
import { dbRead } from "@/lib/api"
import { DB_KEYS } from "@/lib/db-keys"
import { ENGINE_DEFAULTS } from "@/lib/engine-options"

export interface SettingsSnapshot {
    microphoneIndex: string
    wakeWordEngine:  string
    sttEngine:       string
}

export const settingsSnapshot = writable<SettingsSnapshot>({
    microphoneIndex: "-1",
    wakeWordEngine:  ENGINE_DEFAULTS.wakeWordEngine,
    sttEngine:       ENGINE_DEFAULTS.sttEngine,
})

let _loaded = false

export async function loadSettingsSnapshot(): Promise<void> {
    if (_loaded) return
    _loaded = true
    try {
        const [mic, wake, stt] = await Promise.all([
            dbRead(DB_KEYS.microphone),
            dbRead(DB_KEYS.wakeWordEngine),
            dbRead(DB_KEYS.sttEngine),
        ])
        settingsSnapshot.set({
            microphoneIndex: mic  || "-1",
            wakeWordEngine:  wake || ENGINE_DEFAULTS.wakeWordEngine,
            sttEngine:       stt  || ENGINE_DEFAULTS.sttEngine,
        })
    } catch {
        // Non-critical — Stats.svelte falls back to defaults
    }
}

export function invalidateSettingsSnapshot(): void {
    _loaded = false
}
