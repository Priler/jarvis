import { writable } from "svelte/store"
import type { RuntimeEvent } from "@/types"

const EVENTS_MAX = 15
let _nextEventId = 0

export const runtimeEvents = writable<RuntimeEvent[]>([])

export function addRuntimeEvent(title: string, detail = '') {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, '0')
    const time = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
    runtimeEvents.update(evs =>
        [{ id: _nextEventId++, title, detail, time }, ...evs].slice(0, EVENTS_MAX)
    )
}
