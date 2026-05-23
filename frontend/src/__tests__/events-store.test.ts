import { describe, it, expect, beforeEach } from "vitest"
import { get } from "svelte/store"
import { runtimeEvents, addRuntimeEvent } from "@/lib/stores/events"

beforeEach(() => {
    runtimeEvents.set([])
})

describe("addRuntimeEvent", () => {
    it("adds an event to the store", () => {
        addRuntimeEvent("WAKE WORD DETECTED")
        expect(get(runtimeEvents)).toHaveLength(1)
        expect(get(runtimeEvents)[0].title).toBe("WAKE WORD DETECTED")
    })

    it("prepends new events — most recent first", () => {
        addRuntimeEvent("FIRST")
        addRuntimeEvent("SECOND")
        const evs = get(runtimeEvents)
        expect(evs[0].title).toBe("SECOND")
        expect(evs[1].title).toBe("FIRST")
    })

    it("includes detail when provided", () => {
        addRuntimeEvent("COMMAND EXECUTED", "music.play")
        expect(get(runtimeEvents)[0].detail).toBe("music.play")
    })

    it("detail defaults to empty string", () => {
        addRuntimeEvent("SYSTEM IDLE")
        expect(get(runtimeEvents)[0].detail).toBe("")
    })

    it("caps log at 15 entries", () => {
        for (let i = 0; i < 20; i++) addRuntimeEvent(`EVENT ${i}`)
        expect(get(runtimeEvents)).toHaveLength(15)
    })

    it("keeps the most recent events when capped", () => {
        for (let i = 0; i < 20; i++) addRuntimeEvent(`EVENT ${i}`)
        const evs = get(runtimeEvents)
        expect(evs[0].title).toBe("EVENT 19")
        expect(evs[14].title).toBe("EVENT 5")
    })

    it("timestamp matches HH:MM:SS format", () => {
        addRuntimeEvent("TEST")
        expect(get(runtimeEvents)[0].time).toMatch(/^\d{2}:\d{2}:\d{2}$/)
    })

    it("assigns incrementing numeric ids", () => {
        addRuntimeEvent("A")
        addRuntimeEvent("B")
        const [b, a] = get(runtimeEvents)
        expect(b.id).toBeGreaterThan(a.id)
    })
})
