import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"

vi.mock("@/lib/stores/events", () => ({ addRuntimeEvent: vi.fn() }))

import { addRuntimeEvent } from "@/lib/stores/events"
import { jarvisState, lastRecognizedText, lastExecutedCommand, lastError } from "@/lib/ipc"
import { startEventTracking, stopEventTracking } from "@/lib/stores/event-tracker"

const mockAddEvent = vi.mocked(addRuntimeEvent)

beforeEach(() => {
    mockAddEvent.mockClear()
    stopEventTracking()
    jarvisState.set("disconnected")
    lastRecognizedText.set("")
    lastExecutedCommand.set(null)
    lastError.set("")
})

afterEach(() => {
    stopEventTracking()
})

describe("startEventTracking", () => {
    it("does not log initial store values (skipFirst)", () => {
        startEventTracking()
        expect(mockAddEvent).not.toHaveBeenCalled()
    })

    it("logs WAKE WORD DETECTED when state changes to listening", () => {
        startEventTracking()
        jarvisState.set("listening")
        expect(mockAddEvent).toHaveBeenCalledWith("WAKE WORD DETECTED")
    })

    it("logs PROCESSING SPEECH when state changes to processing", () => {
        startEventTracking()
        jarvisState.set("processing")
        expect(mockAddEvent).toHaveBeenCalledWith("PROCESSING SPEECH")
    })

    it("logs SYSTEM IDLE when state changes to idle", () => {
        startEventTracking()
        jarvisState.set("idle")
        expect(mockAddEvent).toHaveBeenCalledWith("SYSTEM IDLE")
    })

    it("logs SPEECH RECOGNIZED with text", () => {
        startEventTracking()
        lastRecognizedText.set("hello world")
        expect(mockAddEvent).toHaveBeenCalledWith("SPEECH RECOGNIZED", "hello world")
    })

    it("logs COMMAND EXECUTED with command id", () => {
        startEventTracking()
        lastExecutedCommand.set({ id: "music.play", seq: 1 })
        expect(mockAddEvent).toHaveBeenCalledWith("COMMAND EXECUTED", "music.play")
    })

    it("logs ERROR with message", () => {
        startEventTracking()
        lastError.set("STT failed")
        expect(mockAddEvent).toHaveBeenCalledWith("ERROR", "STT failed")
    })

    it("is idempotent — calling twice does not double-subscribe", () => {
        startEventTracking()
        startEventTracking()
        jarvisState.set("idle")
        expect(mockAddEvent).toHaveBeenCalledTimes(1)
    })

    it("does not log empty lastError updates", () => {
        startEventTracking()
        lastError.set("")
        expect(mockAddEvent).not.toHaveBeenCalled()
    })

    it("does not log null lastExecutedCommand", () => {
        startEventTracking()
        lastExecutedCommand.set(null)
        expect(mockAddEvent).not.toHaveBeenCalled()
    })
})

describe("stopEventTracking", () => {
    it("unsubscribes all listeners — store changes no longer logged", () => {
        startEventTracking()
        stopEventTracking()
        mockAddEvent.mockClear()
        jarvisState.set("listening")
        expect(mockAddEvent).not.toHaveBeenCalled()
    })

    it("is safe to call without a prior start", () => {
        expect(() => stopEventTracking()).not.toThrow()
    })

    it("allows re-tracking after stop", () => {
        startEventTracking()
        stopEventTracking()
        startEventTracking()
        jarvisState.set("idle")
        expect(mockAddEvent).toHaveBeenCalledWith("SYSTEM IDLE")
    })
})
