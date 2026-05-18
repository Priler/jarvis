import { describe, it, expect, vi, beforeEach } from "vitest"
import { get } from "svelte/store"
import type { Writable } from "svelte/store"

describe("loadAudioDevices", () => {
    let loadAudioDevices: () => Promise<void>
    let invalidateAudioDevices: () => void
    let audioDevices: Writable<string[]>
    let mockGetDevices: ReturnType<typeof vi.fn>

    beforeEach(async () => {
        vi.resetModules()
        mockGetDevices = vi.fn()
        vi.doMock("@/lib/api", () => ({ getAudioDevices: mockGetDevices }))
        const mod = await import("@/lib/stores/audio")
        loadAudioDevices = mod.loadAudioDevices
        invalidateAudioDevices = mod.invalidateAudioDevices
        audioDevices = mod.audioDevices as Writable<string[]>
    })

    it("loads devices from API and updates store", async () => {
        mockGetDevices.mockResolvedValue(["Mic 1", "Mic 2"])
        await loadAudioDevices()
        expect(get(audioDevices)).toEqual(["Mic 1", "Mic 2"])
    })

    it("is idempotent — second call does not call API again", async () => {
        mockGetDevices.mockResolvedValue(["Mic 1"])
        await loadAudioDevices()
        await loadAudioDevices()
        expect(mockGetDevices).toHaveBeenCalledTimes(1)
    })

    it("resets _loaded on error so the next call retries", async () => {
        mockGetDevices.mockRejectedValueOnce(new Error("hardware error"))
        await loadAudioDevices().catch(() => {})
        mockGetDevices.mockResolvedValue(["Mic 1"])
        await loadAudioDevices()
        expect(mockGetDevices).toHaveBeenCalledTimes(2)
    })

    it("throws on API failure so callers can show error feedback", async () => {
        mockGetDevices.mockRejectedValue(new Error("fail"))
        await expect(loadAudioDevices()).rejects.toThrow("fail")
    })

    it("leaves store unchanged on failure", async () => {
        mockGetDevices.mockRejectedValue(new Error("fail"))
        await loadAudioDevices().catch(() => {})
        expect(get(audioDevices)).toEqual([])
    })
})

describe("invalidateAudioDevices", () => {
    let loadAudioDevices: () => Promise<void>
    let invalidateAudioDevices: () => void
    let mockGetDevices: ReturnType<typeof vi.fn>

    beforeEach(async () => {
        vi.resetModules()
        mockGetDevices = vi.fn()
        vi.doMock("@/lib/api", () => ({ getAudioDevices: mockGetDevices }))
        const mod = await import("@/lib/stores/audio")
        loadAudioDevices = mod.loadAudioDevices
        invalidateAudioDevices = mod.invalidateAudioDevices
    })

    it("allows loadAudioDevices to call API again after invalidation", async () => {
        mockGetDevices.mockResolvedValue(["Mic 1"])
        await loadAudioDevices()
        invalidateAudioDevices()
        mockGetDevices.mockResolvedValue(["Mic 1", "Mic 2"])
        await loadAudioDevices()
        expect(mockGetDevices).toHaveBeenCalledTimes(2)
    })
})
