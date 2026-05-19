// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, fireEvent, waitFor } from "@testing-library/svelte"

// ── Shared mock state ─────────────────────────────────────────────────────────
const mocks = vi.hoisted(() => ({
    tFn:                   (key: string) => key,
    addToast:              vi.fn(),
    loadAudioDevices:      vi.fn().mockResolvedValue(undefined),
    loadSettingsPageData:  vi.fn(),
    getSupportedLanguages: vi.fn().mockResolvedValue(["ru", "en", "ua"]),
    saveSettingsValues:    vi.fn().mockResolvedValue(undefined),
    listOllamaModels:      vi.fn().mockResolvedValue([]),
    invalidateSettingsSnapshot: vi.fn(),
    invalidateAudioDevices:     vi.fn(),
}))

// Minimal Svelte 4 component stub — satisfies all internal helpers that the
// parent's compiled code calls: create_component, mount_component,
// destroy_component, transition_in/out, and bind().
function makeSvelteStub() {
    return class SvelteStub {
        $$: Record<string, unknown>
        constructor(_options: unknown) {
            this.$$ = {
                fragment:    { c: () => {}, m: () => {}, p: () => {}, i: () => {}, o: () => {}, d: () => {} },
                on_mount:    [],   // mount_component calls on_mount.map(run)
                on_destroy:  [],
                after_update:[],
                ctx:         [],
                dirty:       [-1],
                skip_bound:  false,
                bound:       {},
                callbacks:   {},
                props:       {},   // bind() reads component.$$.props[name]
                not_equal:   (a: unknown, b: unknown) => a !== b,
            }
        }
        $destroy() { (this.$$ as any).fragment = null; (this.$$ as any).on_destroy = null }
        $set(_props: unknown) {}
        $on(_type: string, _fn: unknown) { return () => {} }
    }
}

vi.mock("@roxi/routify", () => ({
    goto:     { subscribe: (fn: (v: unknown) => void) => { fn(vi.fn()); return () => {} } },
    isActive: { subscribe: (fn: (v: unknown) => void) => { fn(vi.fn(() => false)); return () => {} } },
}))

vi.mock("@/stores", async () => {
    const { writable, readable } = await import("svelte/store")
    return {
        tStore:                  writable(mocks.tFn),
        appInfo:                 readable({ authorName: "", feedbackLink: "", logFilePath: "", tgOfficialLink: "", repositoryLink: "", boostySupportLink: "", patreonSupportLink: "" }),
        assistantVoice:          writable(""),
        currentLanguage:         writable("ru"),
        audioDevices:            writable<string[]>([]),
        settingsSnapshot:        writable({ wakeWordEngine: "vosk", sttEngine: "vosk", microphoneIndex: "" }),
        setLanguage:             vi.fn().mockResolvedValue(undefined),
        loadAudioDevices:        mocks.loadAudioDevices,
        invalidateAudioDevices:  mocks.invalidateAudioDevices,
        invalidateSettingsSnapshot: mocks.invalidateSettingsSnapshot,
        getSupportedLanguages:   mocks.getSupportedLanguages,
    }
})

vi.mock("@/lib/toast", () => ({ addToast: mocks.addToast }))

vi.mock("@/lib/api", () => ({ listOllamaModels: mocks.listOllamaModels }))

vi.mock("@/lib/settings", () => ({ saveSettingsValues: mocks.saveSettingsValues }))

vi.mock("@/lib/settings-loader", () => ({
    loadSettingsPageData: mocks.loadSettingsPageData,
}))

vi.mock("@/lib/engine-options", () => ({
    LANGUAGE_NAMES: { ru: "Русский", en: "English", ua: "Українська" },
    DEFAULT_VOICE_ID: "jarvis",
}))

// Stub heavy tab sub-components so the test only covers Settings navigation logic.
vi.mock("@/components/settings/TabGeneral.svelte", () => ({ default: makeSvelteStub() }))
vi.mock("@/components/settings/TabDevices.svelte", () => ({ default: makeSvelteStub() }))
vi.mock("@/components/settings/TabNeural.svelte",  () => ({ default: makeSvelteStub() }))
vi.mock("@/components/settings/TabAbout.svelte",   () => ({ default: makeSvelteStub() }))
vi.mock("@/components/ui/Button.svelte",           () => ({ default: makeSvelteStub() }))

import SettingsPage from "@/routes/settings/index.svelte"

const SETTINGS_DATA = {
    appVersion:            "0.1.0",
    availableVoices:       [],
    availableVoskModels:   [],
    availableGlinerModels: [],
    settings: {
        microphone:            "-1",
        wakeWordEngine:        "vosk",
        intentEngine:          "none",
        slotEngine:            "none",
        glinerModel:           "",
        voskModel:             "",
        noiseSuppression:      "none",
        vad:                   "energy",
        vadEnergyThreshold:    100,
        gainNormalizerEnabled: false,
        apiKeyPicovoice:       "",
        ollamaUrl:             "http://localhost:11434",
        ollamaModel:           "",
    },
    errors: {
        meta:     false,
        voices:   false,
        vosk:     false,
        gliner:   false,
        settings: false,
    },
}

beforeEach(() => {
    vi.clearAllMocks()
    mocks.loadSettingsPageData.mockResolvedValue(SETTINGS_DATA)
    mocks.loadAudioDevices.mockResolvedValue(undefined)
    mocks.getSupportedLanguages.mockResolvedValue(["ru", "en", "ua"])
})

async function renderLoaded() {
    const result = render(SettingsPage)
    // Wait for onMount to finish and loading skeleton to disappear
    await waitFor(() => expect(result.getAllByRole("tab")).toHaveLength(4), { timeout: 3000 })
    return result
}

describe("Settings page — tab navigation", () => {
    it("renders exactly 4 tab buttons once loaded", async () => {
        const { getAllByRole } = await renderLoaded()
        expect(getAllByRole("tab")).toHaveLength(4)
    })

    it("general tab is selected by default", async () => {
        const { getAllByRole } = await renderLoaded()
        const [general] = getAllByRole("tab")
        expect(general.getAttribute("aria-selected")).toBe("true")
    })

    it("clicking a tab changes aria-selected", async () => {
        const { getAllByRole } = await renderLoaded()
        const tabs = getAllByRole("tab")
        await fireEvent.click(tabs[1]) // devices
        expect(tabs[1].getAttribute("aria-selected")).toBe("true")
        expect(tabs[0].getAttribute("aria-selected")).toBe("false")
    })

    it("ArrowRight moves selection to next tab", async () => {
        const { getAllByRole, getByRole } = await renderLoaded()
        const tablist = getByRole("tablist")
        await fireEvent.keyDown(tablist, { key: "ArrowRight" })
        const tabs = getAllByRole("tab")
        expect(tabs[1].getAttribute("aria-selected")).toBe("true")
    })

    it("ArrowLeft wraps from first to last tab", async () => {
        const { getAllByRole, getByRole } = await renderLoaded()
        const tablist = getByRole("tablist")
        await fireEvent.keyDown(tablist, { key: "ArrowLeft" })
        const tabs = getAllByRole("tab")
        expect(tabs[3].getAttribute("aria-selected")).toBe("true")
    })

    it("tabpanel has correct aria-labelledby pointing to active tab", async () => {
        const { getByRole } = await renderLoaded()
        const panel = getByRole("tabpanel")
        expect(panel.getAttribute("aria-labelledby")).toContain("general")
    })
})
