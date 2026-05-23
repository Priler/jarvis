import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { get } from "svelte/store"

const flush = async () => {
    for (let i = 0; i < 5; i++) await Promise.resolve()
}

vi.mock("@/lib/api", () => ({
    getJarvisStats: vi.fn(),
}))

// Stub document — not available in node environment
const mockDocument = {
    hidden: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
}
vi.stubGlobal("document", mockDocument)

import { getJarvisStats } from "@/lib/api"
import {
    jarvisStats,
    isJarvisRunning,
    jarvisRamUsage,
    jarvisCpuUsage,
    updateJarvisStats,
    startStatsPolling,
    stopStatsPolling,
} from "@/lib/stores/runtime"

const mockGetStats = vi.mocked(getJarvisStats)

const RESET = { running: false, ram_mb: 0, cpu_usage: 0 }

// ── updateJarvisStats ─────────────────────────────────────────────────────────

describe("updateJarvisStats", () => {
    beforeEach(() => {
        mockGetStats.mockReset()
        jarvisStats.set(RESET)
    })

    it("updates all stores on success", async () => {
        mockGetStats.mockResolvedValueOnce({ running: true, ram_mb: 256, cpu_usage: 12.3 })
        await updateJarvisStats()
        expect(get(isJarvisRunning)).toBe(true)
        expect(get(jarvisRamUsage)).toBe(256)
        expect(get(jarvisCpuUsage)).toBe(12.3)
    })

    it("sets running=false when API returns false", async () => {
        jarvisStats.set({ running: true, ram_mb: 0, cpu_usage: 0 })
        mockGetStats.mockResolvedValueOnce({ running: false, ram_mb: 0, cpu_usage: 0 })
        await updateJarvisStats()
        expect(get(isJarvisRunning)).toBe(false)
    })

    it("does not throw on API failure", async () => {
        mockGetStats.mockRejectedValueOnce(new Error("IPC error"))
        await expect(updateJarvisStats()).resolves.toBeUndefined()
    })

    it("leaves stores unchanged on API failure", async () => {
        jarvisStats.set({ running: true, ram_mb: 64, cpu_usage: 0 })
        mockGetStats.mockRejectedValueOnce(new Error("IPC error"))
        await updateJarvisStats()
        expect(get(isJarvisRunning)).toBe(true)
        expect(get(jarvisRamUsage)).toBe(64)
    })
})

// ── startStatsPolling / stopStatsPolling ──────────────────────────────────────

describe("startStatsPolling / stopStatsPolling", () => {
    beforeEach(() => {
        vi.useFakeTimers()
        mockGetStats.mockReset()
        mockGetStats.mockResolvedValue(RESET)
        mockDocument.addEventListener.mockClear()
        mockDocument.removeEventListener.mockClear()
        stopStatsPolling() // reset module-level interval state
    })

    afterEach(() => {
        stopStatsPolling()
        vi.useRealTimers()
    })

    it("calls updateJarvisStats immediately on start", async () => {
        startStatsPolling(1000)
        await flush()
        expect(mockGetStats).toHaveBeenCalledTimes(1)
    })

    it("polls again on each interval tick", async () => {
        startStatsPolling(1000)
        await flush()
        vi.advanceTimersByTime(1000)
        await flush()
        vi.advanceTimersByTime(1000)
        await flush()
        expect(mockGetStats.mock.calls.length).toBeGreaterThanOrEqual(3)
    })

    it("is idempotent — a second start call is a no-op", async () => {
        startStatsPolling(1000)
        startStatsPolling(1000) // should not create a second interval
        await flush()
        vi.advanceTimersByTime(1000)
        await flush()
        // With two intervals we'd expect 4+ calls; with one, 2
        expect(mockGetStats.mock.calls.length).toBeLessThanOrEqual(3)
    })

    it("stops polling after stopStatsPolling", async () => {
        startStatsPolling(1000)
        await flush()
        const callsBeforeStop = mockGetStats.mock.calls.length
        stopStatsPolling()
        vi.advanceTimersByTime(5000)
        await flush()
        expect(mockGetStats.mock.calls.length).toBe(callsBeforeStop)
    })

    it("registers a visibilitychange listener on start", () => {
        startStatsPolling(1000)
        expect(mockDocument.addEventListener).toHaveBeenCalledWith(
            "visibilitychange",
            expect.any(Function)
        )
    })

    it("removes the visibilitychange listener on stop", () => {
        startStatsPolling(1000)
        stopStatsPolling()
        expect(mockDocument.removeEventListener).toHaveBeenCalledWith(
            "visibilitychange",
            expect.any(Function)
        )
    })
})
