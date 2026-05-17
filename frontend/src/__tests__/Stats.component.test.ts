// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, waitFor, act } from "@testing-library/svelte"
import type { Writable } from "svelte/store"

// Mock functions are hoisted so the vi.mock factory can reference them
const mocks = vi.hoisted(() => ({
    loadAudioDevices:     vi.fn().mockResolvedValue(undefined),
    loadSettingsSnapshot: vi.fn().mockResolvedValue(undefined),
    // Store refs populated when the factory first runs
    isJarvisRunning:      null as Writable<boolean> | null,
    jarvisRamUsage:       null as Writable<number> | null,
    audioDevices:         null as Writable<string[]> | null,
    settingsSnapshot:     null as Writable<{ wakeWordEngine: string; sttEngine: string; microphoneIndex: string }> | null,
}))

// Use real svelte/store writable (same as SearchBar tests) so Svelte's
// store subscription + invalidation protocol works correctly with onMount.
vi.mock("@/stores", async () => {
    const { writable } = await import("svelte/store")

    const isJarvisRunning  = writable(false)
    const jarvisRamUsage   = writable(0)
    const audioDevices     = writable<string[]>([])
    const settingsSnapshot = writable({ wakeWordEngine: "vosk", sttEngine: "vosk", microphoneIndex: "" })
    const tStore           = writable((k: string, fb = k) => fb)

    // Expose references so beforeEach can reset them
    mocks.isJarvisRunning  = isJarvisRunning
    mocks.jarvisRamUsage   = jarvisRamUsage
    mocks.audioDevices     = audioDevices
    mocks.settingsSnapshot = settingsSnapshot

    return {
        isJarvisRunning,
        jarvisRamUsage,
        tStore,
        audioDevices,
        settingsSnapshot,
        loadAudioDevices:     mocks.loadAudioDevices,
        loadSettingsSnapshot: mocks.loadSettingsSnapshot,
    }
})

import Stats from "@/components/elements/Stats.svelte"

beforeEach(() => {
    mocks.isJarvisRunning?.set(false)
    mocks.jarvisRamUsage?.set(0)
    mocks.audioDevices?.set([])
    mocks.settingsSnapshot?.set({ wakeWordEngine: "vosk", sttEngine: "vosk", microphoneIndex: "" })
    mocks.loadAudioDevices.mockClear()
    mocks.loadSettingsSnapshot.mockClear()
})

describe("Stats", () => {
    it("shows — for resources when jarvis is not running", () => {
        const { container } = render(Stats)
        expect(container.textContent).toContain("—")
    })

    it("shows RAM usage when jarvis is running", () => {
        mocks.isJarvisRunning?.set(true)
        mocks.jarvisRamUsage?.set(256)
        const { container } = render(Stats)
        expect(container.textContent).toContain("RAM 256MB")
    })

    it("shows single engine label when wakeWordEngine equals sttEngine", () => {
        mocks.settingsSnapshot?.set({ wakeWordEngine: "vosk", sttEngine: "vosk", microphoneIndex: "" })
        const { container } = render(Stats)
        expect(container.textContent).toContain("vosk")
    })

    it("shows combined engine label when wakeWordEngine differs from sttEngine", () => {
        mocks.settingsSnapshot?.set({ wakeWordEngine: "oww", sttEngine: "whisper", microphoneIndex: "" })
        const { container } = render(Stats)
        expect(container.textContent).toContain("oww + whisper")
    })

    it("calls loadAudioDevices and loadSettingsSnapshot on mount", () => {
        render(Stats)
        // loadAudioDevices() is called synchronously inside onMount before the first await
        expect(mocks.loadAudioDevices).toHaveBeenCalled()
        expect(mocks.loadSettingsSnapshot).toHaveBeenCalled()
    })

    it("resolves microphone name by device index", async () => {
        mocks.audioDevices?.set(["System Default", "USB Mic", "Built-in"])
        mocks.settingsSnapshot?.set({ wakeWordEngine: "vosk", sttEngine: "vosk", microphoneIndex: "1" })
        const { container } = render(Stats)
        await waitFor(() => expect(container.textContent).toContain("USB Mic"), { timeout: 2000 })
    })

    it("shows system default label when microphoneIndex is -1", async () => {
        mocks.audioDevices?.set(["USB Mic"])
        mocks.settingsSnapshot?.set({ wakeWordEngine: "vosk", sttEngine: "vosk", microphoneIndex: "-1" })
        const { container } = render(Stats)
        // "stats-system-default" is 20 chars; truncate(_, 18) renders "stats-system-defau..."
        await waitFor(() => expect(container.textContent).toContain("stats-system-defau..."), { timeout: 2000 })
    })

    it("truncates long microphone names to 18 characters", async () => {
        mocks.audioDevices?.set(["A very long microphone name that exceeds limit"])
        mocks.settingsSnapshot?.set({ wakeWordEngine: "vosk", sttEngine: "vosk", microphoneIndex: "0" })
        const { container } = render(Stats)
        // truncate(name, 18) takes the first 18 chars then appends "..."
        await waitFor(() => expect(container.textContent).toContain("A very long microp..."), { timeout: 2000 })
    })
})
