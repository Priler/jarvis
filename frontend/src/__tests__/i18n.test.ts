import { describe, it, expect, vi, beforeEach } from "vitest"
import { get } from "svelte/store"

vi.mock("@/lib/api", () => ({
    getTranslations:    vi.fn(),
    getCurrentLanguage: vi.fn(),
    setLanguageInvoke:  vi.fn(),
    getSupportedLangs:  vi.fn(),
}))
vi.mock("@/lib/toast", () => ({ addToast: vi.fn() }))

import { getTranslations, getCurrentLanguage, setLanguageInvoke, getSupportedLangs } from "@/lib/api"
import {
    translate,
    translations,
    currentLanguage,
    tStore,
    loadTranslations,
    setLanguage,
    getSupportedLanguages,
} from "@/lib/i18n"

const mockGetTrans    = vi.mocked(getTranslations)
const mockGetLang     = vi.mocked(getCurrentLanguage)
const mockSetLang     = vi.mocked(setLanguageInvoke)
const mockGetSupported = vi.mocked(getSupportedLangs)

beforeEach(() => {
    vi.clearAllMocks()
    translations.set({})
    currentLanguage.set("ru")
})

describe("translate", () => {
    it("returns the translation for a key", () => {
        expect(translate({ hello: "Привет" }, "hello")).toBe("Привет")
    })

    it("returns the fallback when key is missing", () => {
        expect(translate({}, "missing", "Default")).toBe("Default")
    })

    it("returns the key itself when no fallback and key is missing", () => {
        expect(translate({}, "missing-key")).toBe("missing-key")
    })
})

describe("tStore", () => {
    it("returns a function that translates using current translations", () => {
        translations.set({ greeting: "Hello" })
        expect(get(tStore)("greeting")).toBe("Hello")
    })

    it("accepts an optional fallback argument", () => {
        translations.set({})
        expect(get(tStore)("missing", "Fallback")).toBe("Fallback")
    })

    it("updates when translations store changes", () => {
        translations.set({ key: "old" })
        expect(get(tStore)("key")).toBe("old")
        translations.set({ key: "new" })
        expect(get(tStore)("key")).toBe("new")
    })
})

describe("loadTranslations", () => {
    it("sets translations and current language from API", async () => {
        mockGetTrans.mockResolvedValue({ hello: "Привет" })
        mockGetLang.mockResolvedValue("ru")
        await loadTranslations()
        expect(get(translations)).toEqual({ hello: "Привет" })
        expect(get(currentLanguage)).toBe("ru")
    })

    it("does not throw on API failure", async () => {
        mockGetTrans.mockRejectedValue(new Error("network"))
        mockGetLang.mockRejectedValue(new Error("network"))
        await expect(loadTranslations()).resolves.toBeUndefined()
    })
})

describe("setLanguage", () => {
    it("updates translations and currentLanguage on success", async () => {
        mockSetLang.mockResolvedValue({ key: "value" })
        await setLanguage("en")
        expect(get(translations)).toEqual({ key: "value" })
        expect(get(currentLanguage)).toBe("en")
    })

    it("does not throw on failure", async () => {
        mockSetLang.mockRejectedValue(new Error("fail"))
        await expect(setLanguage("en")).resolves.toBeUndefined()
    })
})

describe("getSupportedLanguages", () => {
    it("returns language codes from API", async () => {
        mockGetSupported.mockResolvedValue(["ru", "en", "ua"])
        expect(await getSupportedLanguages()).toEqual(["ru", "en", "ua"])
    })

    it("returns hard-coded fallback on failure", async () => {
        mockGetSupported.mockRejectedValue(new Error("fail"))
        const langs = await getSupportedLanguages()
        expect(langs).toContain("ru")
        expect(langs).toContain("en")
    })
})
