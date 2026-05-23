import { describe, it, expect } from "vitest"
import { parseIpcMessage, computeReconnectDelay } from "@/lib/ipc/utils"

describe("parseIpcMessage", () => {
    it("parses a valid event message", () => {
        const result = parseIpcMessage(JSON.stringify({ event: "idle" }))
        expect(result).toEqual({ event: "idle" })
    })

    it("parses speech_recognized with text", () => {
        const result = parseIpcMessage(JSON.stringify({ event: "speech_recognized", text: "turn on lights" }))
        expect(result).toEqual({ event: "speech_recognized", text: "turn on lights" })
    })

    it("parses command_executed with id", () => {
        const result = parseIpcMessage(JSON.stringify({ event: "command_executed", id: "lights.on" }))
        expect(result).toEqual({ event: "command_executed", id: "lights.on" })
    })

    it("returns null for invalid JSON", () => {
        expect(parseIpcMessage("not json")).toBeNull()
        expect(parseIpcMessage("{broken")).toBeNull()
        expect(parseIpcMessage("")).toBeNull()
    })

    it("returns null when event field is missing", () => {
        expect(parseIpcMessage(JSON.stringify({ action: "ping" }))).toBeNull()
        expect(parseIpcMessage(JSON.stringify({}))).toBeNull()
    })

    it("returns null when event field is not a string", () => {
        expect(parseIpcMessage(JSON.stringify({ event: 42 }))).toBeNull()
        expect(parseIpcMessage(JSON.stringify({ event: null }))).toBeNull()
        expect(parseIpcMessage(JSON.stringify({ event: true }))).toBeNull()
    })

    it("returns null for non-object JSON", () => {
        expect(parseIpcMessage(JSON.stringify(null))).toBeNull()
        expect(parseIpcMessage(JSON.stringify([1, 2]))).toBeNull()
        expect(parseIpcMessage(JSON.stringify("string"))).toBeNull()
    })
})

describe("computeReconnectDelay", () => {
    it("returns base delay on first attempt", () => {
        expect(computeReconnectDelay(0, 1000, 3000)).toBe(1000)
    })

    it("doubles delay each attempt", () => {
        expect(computeReconnectDelay(1, 1000, 3000)).toBe(2000)
        expect(computeReconnectDelay(2, 1000, 3000)).toBe(3000)
    })

    it("caps at maxMs", () => {
        expect(computeReconnectDelay(3, 1000, 3000)).toBe(3000)
        expect(computeReconnectDelay(10, 1000, 3000)).toBe(3000)
        expect(computeReconnectDelay(100, 1000, 3000)).toBe(3000)
    })

    it("uses default values matching production constants", () => {
        expect(computeReconnectDelay(0)).toBe(1000)
        expect(computeReconnectDelay(1)).toBe(2000)
        expect(computeReconnectDelay(5)).toBe(3000)
    })

    it("works with custom base and max", () => {
        expect(computeReconnectDelay(0, 500, 2000)).toBe(500)
        expect(computeReconnectDelay(2, 500, 2000)).toBe(2000)
    })
})
