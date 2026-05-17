import { writable } from "svelte/store"
import { getAudioDevices } from "@/lib/api"

export const audioDevices = writable<string[]>([])
let _loaded = false

export async function loadAudioDevices() {
    if (_loaded) return
    _loaded = true
    try {
        const devices = await getAudioDevices()
        audioDevices.set(devices)
    } catch (err: unknown) {
        _loaded = false
        console.error("failed to load audio devices:", err)
        throw err
    }
}
