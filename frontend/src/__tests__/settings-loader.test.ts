import { describe, it, expect, vi, beforeEach } from "vitest"

const {
    mockGetAppVersion,
    mockListVoices,
    mockListVoskModels,
    mockListGlinerModels,
    mockLoadSettingsValues,
} = vi.hoisted(() => ({
    mockGetAppVersion:       vi.fn(),
    mockListVoices:          vi.fn(),
    mockListVoskModels:      vi.fn(),
    mockListGlinerModels:    vi.fn(),
    mockLoadSettingsValues:  vi.fn(),
}))

vi.mock("@/lib/api",      () => ({
    getAppVersion:    mockGetAppVersion,
    listVoices:       mockListVoices,
    listVoskModels:   mockListVoskModels,
    listGlinerModels: mockListGlinerModels,
}))
vi.mock("@/lib/settings", () => ({ loadSettingsValues: mockLoadSettingsValues }))

import { loadSettingsPageData } from "@/lib/settings-loader"
import type { SettingsValues } from "@/lib/settings"

const SETTINGS_STUB: SettingsValues = {
    microphone: "0", wakeWordEngine: "Rustpotter", intentEngine: "none",
    slotEngine: "None", glinerModel: "", voskModel: "vosk-ru",
    noiseSuppression: "None", vad: "None", gainNormalizerEnabled: false,
    vadEnergyThreshold: 100,
    apiKeyPicovoice: "", ollamaUrl: "http://localhost:11434", ollamaModel: "",
}

beforeEach(() => {
    vi.clearAllMocks()
    mockGetAppVersion.mockResolvedValue("1.2.3")
    mockListVoices.mockResolvedValue([
        { voice: { id: "jarvis-og", name: "Jarvis OG", author: "me", languages: ["en"] } },
    ])
    mockListVoskModels.mockResolvedValue([
        { name: "vosk-ru-small", language: "ru", size: "45MB" },
    ])
    mockListGlinerModels.mockResolvedValue([
        { display_name: "GLiNER v2", value: "gliner-v2" },
    ])
    mockLoadSettingsValues.mockResolvedValue(SETTINGS_STUB)
})

describe("loadSettingsPageData — happy path", () => {
    it("returns appVersion from getAppVersion", async () => {
        const data = await loadSettingsPageData()
        expect(data.appVersion).toBe("1.2.3")
    })

    it("maps VoiceConfig to VoiceMeta via .voice property", async () => {
        const data = await loadSettingsPageData()
        expect(data.availableVoices).toEqual([
            { id: "jarvis-og", name: "Jarvis OG", author: "me", languages: ["en"] },
        ])
    })

    it("formats Vosk model label with translated language name and size", async () => {
        const data = await loadSettingsPageData()
        expect(data.availableVoskModels).toEqual([
            { label: "vosk-ru-small (Русский, 45MB)", value: "vosk-ru-small" },
        ])
    })

    it("falls back to raw language code when LANGUAGE_NAMES has no entry", async () => {
        mockListVoskModels.mockResolvedValue([
            { name: "vosk-xx", language: "xx", size: "10MB" },
        ])
        const data = await loadSettingsPageData()
        expect(data.availableVoskModels[0].label).toBe("vosk-xx (xx, 10MB)")
    })

    it("maps GLiNER models to SelectOption", async () => {
        const data = await loadSettingsPageData()
        expect(data.availableGlinerModels).toEqual([
            { label: "GLiNER v2", value: "gliner-v2" },
        ])
    })

    it("returns settings from loadSettingsValues", async () => {
        const data = await loadSettingsPageData()
        expect(data.settings).toEqual(SETTINGS_STUB)
    })

    it("reports no errors when all APIs succeed", async () => {
        const data = await loadSettingsPageData()
        expect(data.errors).toEqual({
            meta: false, voices: false, vosk: false, gliner: false, settings: false,
        })
    })
})

describe("loadSettingsPageData — partial failures", () => {
    it("sets errors.meta and returns empty appVersion when getAppVersion rejects", async () => {
        mockGetAppVersion.mockRejectedValue(new Error("net"))
        const data = await loadSettingsPageData()
        expect(data.errors.meta).toBe(true)
        expect(data.appVersion).toBe("")
    })

    it("sets errors.voices and returns [] when listVoices rejects", async () => {
        mockListVoices.mockRejectedValue(new Error("net"))
        const data = await loadSettingsPageData()
        expect(data.errors.voices).toBe(true)
        expect(data.availableVoices).toEqual([])
    })

    it("sets errors.vosk and returns [] when listVoskModels rejects", async () => {
        mockListVoskModels.mockRejectedValue(new Error("net"))
        const data = await loadSettingsPageData()
        expect(data.errors.vosk).toBe(true)
        expect(data.availableVoskModels).toEqual([])
    })

    it("sets errors.gliner and returns [] when listGlinerModels rejects", async () => {
        mockListGlinerModels.mockRejectedValue(new Error("net"))
        const data = await loadSettingsPageData()
        expect(data.errors.gliner).toBe(true)
        expect(data.availableGlinerModels).toEqual([])
    })

    it("sets errors.settings and returns null when loadSettingsValues rejects", async () => {
        mockLoadSettingsValues.mockRejectedValue(new Error("db"))
        const data = await loadSettingsPageData()
        expect(data.errors.settings).toBe(true)
        expect(data.settings).toBeNull()
    })

    it("handles all APIs failing simultaneously", async () => {
        mockGetAppVersion.mockRejectedValue(new Error())
        mockListVoices.mockRejectedValue(new Error())
        mockListVoskModels.mockRejectedValue(new Error())
        mockListGlinerModels.mockRejectedValue(new Error())
        mockLoadSettingsValues.mockRejectedValue(new Error())
        const data = await loadSettingsPageData()
        expect(data.errors).toEqual({
            meta: true, voices: true, vosk: true, gliner: true, settings: true,
        })
        expect(data.appVersion).toBe("")
        expect(data.availableVoices).toEqual([])
        expect(data.settings).toBeNull()
    })
})
