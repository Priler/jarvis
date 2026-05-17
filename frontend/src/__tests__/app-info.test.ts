import { describe, it, expect, vi, beforeEach } from "vitest"
import { get } from "svelte/store"
import type { Writable } from "svelte/store"
import type { AppInfo } from "@/types"

describe("loadAppInfo", () => {
    let loadAppInfo: () => Promise<void>
    let appInfo: Writable<AppInfo>
    let mockApi: Record<string, ReturnType<typeof vi.fn>>

    beforeEach(async () => {
        vi.resetModules()
        mockApi = {
            getAuthorName:     vi.fn().mockResolvedValue("TestAuthor"),
            getTgOfficialLink: vi.fn().mockResolvedValue("https://t.me/test"),
            getFeedbackLink:   vi.fn().mockResolvedValue("https://feedback.test"),
            getRepositoryLink: vi.fn().mockResolvedValue("https://github.com/test"),
            getBoostyLink:     vi.fn().mockResolvedValue("https://boosty.test"),
            getPatreonLink:    vi.fn().mockResolvedValue("https://patreon.test"),
            getLogFilePath:    vi.fn().mockResolvedValue("/tmp/jarvis.log"),
        }
        vi.doMock("@/lib/api", () => mockApi)
        const mod = await import("@/lib/stores/app-info")
        loadAppInfo = mod.loadAppInfo
        appInfo     = mod.appInfo as Writable<AppInfo>
    })

    it("loads all fields from API", async () => {
        await loadAppInfo()
        const info = get(appInfo)
        expect(info.authorName).toBe("TestAuthor")
        expect(info.tgOfficialLink).toBe("https://t.me/test")
        expect(info.logFilePath).toBe("/tmp/jarvis.log")
    })

    it("is idempotent — second call does not call API again", async () => {
        await loadAppInfo()
        await loadAppInfo()
        expect(mockApi.getAuthorName).toHaveBeenCalledTimes(1)
    })

    it("uses empty string for a field whose API call fails (allSettled)", async () => {
        mockApi.getAuthorName.mockRejectedValue(new Error("fail"))
        await loadAppInfo()
        const info = get(appInfo)
        expect(info.authorName).toBe("")
        expect(info.tgOfficialLink).toBe("https://t.me/test")
    })

    it("does not throw even if all API calls fail", async () => {
        Object.values(mockApi).forEach(fn => fn.mockRejectedValue(new Error("fail")))
        await expect(loadAppInfo()).resolves.toBeUndefined()
    })

    it("sets all fields to empty string when all API calls fail", async () => {
        Object.values(mockApi).forEach(fn => fn.mockRejectedValue(new Error("fail")))
        await loadAppInfo()
        const info = get(appInfo)
        expect(info.authorName).toBe("")
        expect(info.logFilePath).toBe("")
    })
})
