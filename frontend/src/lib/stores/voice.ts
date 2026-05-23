import { writable } from "svelte/store"
import { addToast } from "@/lib/toast"
import { dbRead } from "@/lib/api"
import { DB_KEYS } from "@/lib/db-keys"

export const assistantVoice = writable("")

let _loaded = false

export async function loadVoiceSetting() {
    if (_loaded) return
    _loaded = true
    try {
        const voice = await dbRead(DB_KEYS.voice)
        assistantVoice.set(voice)
    } catch (err: unknown) {
        _loaded = false
        console.error("failed to load voice setting:", err)
        addToast("Failed to load voice setting", "error")
    }
}
