import { describe, it, expect, vi, beforeEach } from "vitest"
import { get } from "svelte/store"
import type { Writable } from "svelte/store"

const mockAddToast = vi.fn()

describe("loadVoiceSetting", () => {
    let loadVoiceSetting: () => Promise<void>
    let assistantVoice: Writable<string>
    let mockDbRead: ReturnType<typeof vi.fn>

    beforeEach(async () => {
        vi.resetModules()
        mockDbRead = vi.fn()
        mockAddToast.mockClear()
        vi.doMock("@/lib/api",   () => ({ dbRead: mockDbRead }))
        vi.doMock("@/lib/toast", () => ({ addToast: mockAddToast }))
        const mod = await import("@/lib/stores/voice")
        loadVoiceSetting = mod.loadVoiceSetting
        assistantVoice   = mod.assistantVoice as Writable<string>
    })

    it("loads voice from DB and updates store", async () => {
        mockDbRead.mockResolvedValue("en-female")
        await loadVoiceSetting()
        expect(get(assistantVoice)).toBe("en-female")
    })

    it("is idempotent — second call does not hit DB again", async () => {
        mockDbRead.mockResolvedValue("en-male")
        await loadVoiceSetting()
        await loadVoiceSetting()
        expect(mockDbRead).toHaveBeenCalledTimes(1)
    })

    it("resets _loaded on error so the next call retries", async () => {
        mockDbRead.mockRejectedValueOnce(new Error("DB error"))
        await loadVoiceSetting()
        mockDbRead.mockResolvedValue("ru-female")
        await loadVoiceSetting()
        expect(mockDbRead).toHaveBeenCalledTimes(2)
    })

    it("shows an error toast on failure", async () => {
        mockDbRead.mockRejectedValue(new Error("fail"))
        await loadVoiceSetting()
        expect(mockAddToast).toHaveBeenCalled()
    })

    it("does not throw on failure", async () => {
        mockDbRead.mockRejectedValue(new Error("fail"))
        await expect(loadVoiceSetting()).resolves.toBeUndefined()
    })
})
