import { describe, it, expect, vi, beforeEach } from "vitest"
import { get } from "svelte/store"

vi.mock("@/lib/api", () => ({
    dbRead: vi.fn(),
}))

import { dbRead } from "@/lib/api"
import {
    settingsSnapshot,
    loadSettingsSnapshot,
    invalidateSettingsSnapshot,
} from "@/lib/stores/settings-cache"
import { ENGINE_DEFAULTS } from "@/lib/engine-options"

const mockDbRead = vi.mocked(dbRead)

beforeEach(() => {
    mockDbRead.mockReset()
    invalidateSettingsSnapshot()
    settingsSnapshot.set({
        microphoneIndex: "-1",
        wakeWordEngine:  ENGINE_DEFAULTS.wakeWordEngine,
        sttEngine:       ENGINE_DEFAULTS.sttEngine,
    })
})

describe("loadSettingsSnapshot", () => {
    it("loads values from DB and updates the store", async () => {
        mockDbRead
            .mockResolvedValueOnce("3")            // microphone
            .mockResolvedValueOnce("Picovoice")    // wakeWordEngine
            .mockResolvedValueOnce("Vosk")         // sttEngine
        await loadSettingsSnapshot()
        const snap = get(settingsSnapshot)
        expect(snap.microphoneIndex).toBe("3")
        expect(snap.wakeWordEngine).toBe("Picovoice")
        expect(snap.sttEngine).toBe("Vosk")
    })

    it("falls back to -1 for mic when DB returns empty string", async () => {
        mockDbRead.mockResolvedValue("")
        await loadSettingsSnapshot()
        expect(get(settingsSnapshot).microphoneIndex).toBe("-1")
    })

    it("falls back to engine defaults when DB returns empty strings", async () => {
        mockDbRead.mockResolvedValue("")
        await loadSettingsSnapshot()
        const snap = get(settingsSnapshot)
        expect(snap.wakeWordEngine).toBe(ENGINE_DEFAULTS.wakeWordEngine)
        expect(snap.sttEngine).toBe(ENGINE_DEFAULTS.sttEngine)
    })

    it("is idempotent — second call without invalidation is a no-op", async () => {
        mockDbRead.mockResolvedValue("x")
        await loadSettingsSnapshot()
        await loadSettingsSnapshot()
        // 3 DB reads total, not 6
        expect(mockDbRead).toHaveBeenCalledTimes(3)
    })

    it("silently recovers without throwing on DB failure", async () => {
        mockDbRead.mockRejectedValue(new Error("DB unavailable"))
        await expect(loadSettingsSnapshot()).resolves.toBeUndefined()
    })

    it("leaves defaults in place after DB failure", async () => {
        mockDbRead.mockRejectedValue(new Error("DB unavailable"))
        await loadSettingsSnapshot()
        const snap = get(settingsSnapshot)
        expect(snap.microphoneIndex).toBe("-1")
        expect(snap.wakeWordEngine).toBe(ENGINE_DEFAULTS.wakeWordEngine)
    })
})

describe("invalidateSettingsSnapshot", () => {
    it("allows a second loadSettingsSnapshot call to re-fetch from DB", async () => {
        mockDbRead.mockResolvedValue("first")
        await loadSettingsSnapshot()

        invalidateSettingsSnapshot()

        mockDbRead.mockReset()
        mockDbRead.mockResolvedValue("second")
        await loadSettingsSnapshot()

        // 3 new DB reads happened after invalidation
        expect(mockDbRead).toHaveBeenCalledTimes(3)
    })

    it("updates the store with fresh data after invalidation", async () => {
        mockDbRead.mockResolvedValue("")
        await loadSettingsSnapshot()

        invalidateSettingsSnapshot()

        mockDbRead.mockResolvedValue("7")
        await loadSettingsSnapshot()

        expect(get(settingsSnapshot).microphoneIndex).toBe("7")
    })
})
