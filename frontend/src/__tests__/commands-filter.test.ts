import { describe, it, expect } from "vitest"
import { getPhrases, filterCommands } from "@/lib/commands-filter"
import type { JCommand } from "@/types"

// ── Helpers ──────────────────────────────────────────────────────────────────

function makeCmd(id: string, phrases: Record<string, string[]>, type = "lua"): JCommand {
    return { id, type, description: "", phrases, slots: {} }
}

const COMMANDS: JCommand[] = [
    makeCmd("lights.on",  { ru: ["включи свет", "свет on"], en: ["turn on lights", "lights on"] }),
    makeCmd("lights.off", { ru: ["выключи свет"],           en: ["turn off lights", "lights off"] }),
    makeCmd("play.music", { ru: ["включи музыку"],          en: ["play music", "start music"] }),
    makeCmd("weather",    { ru: ["погода"],                 en: ["weather", "what's the weather"] }),
]

// ── getPhrases ────────────────────────────────────────────────────────────────

describe("getPhrases", () => {
    it("returns phrases for the requested language", () => {
        const cmd = makeCmd("test", { ru: ["привет"], en: ["hello"] })
        expect(getPhrases(cmd, "ru")).toEqual(["привет"])
        expect(getPhrases(cmd, "en")).toEqual(["hello"])
    })

    it("falls back to English when requested lang is missing", () => {
        const cmd = makeCmd("test", { en: ["hello"] })
        expect(getPhrases(cmd, "ru")).toEqual(["hello"])
    })

    it("falls back to first available language when English is also missing", () => {
        const cmd = makeCmd("test", { de: ["hallo"] })
        expect(getPhrases(cmd, "ru")).toEqual(["hallo"])
        expect(getPhrases(cmd, "en")).toEqual(["hallo"])
    })

    it("returns empty array when no phrases exist", () => {
        const cmd = makeCmd("test", {})
        expect(getPhrases(cmd, "ru")).toEqual([])
    })
})

// ── filterCommands ────────────────────────────────────────────────────────────

describe("filterCommands", () => {
    it("returns all commands when query is empty", () => {
        expect(filterCommands(COMMANDS, "", "ru")).toBe(COMMANDS)
        expect(filterCommands(COMMANDS, "   ", "ru")).toBe(COMMANDS)
    })

    it("filters by command id (case-insensitive)", () => {
        const result = filterCommands(COMMANDS, "lights", "ru")
        expect(result).toHaveLength(2)
        expect(result.map(c => c.id)).toContain("lights.on")
        expect(result.map(c => c.id)).toContain("lights.off")
    })

    it("filters by phrase in current language", () => {
        const result = filterCommands(COMMANDS, "свет", "ru")
        expect(result).toHaveLength(2)
        expect(result.map(c => c.id)).toContain("lights.on")
        expect(result.map(c => c.id)).toContain("lights.off")
    })

    it("filters by English phrase when lang is en", () => {
        const result = filterCommands(COMMANDS, "music", "en")
        expect(result).toHaveLength(1)
        expect(result[0].id).toBe("play.music")
    })

    it("is case-insensitive", () => {
        expect(filterCommands(COMMANDS, "LIGHTS", "en")).toHaveLength(2)
        expect(filterCommands(COMMANDS, "Lights", "en")).toHaveLength(2)
    })

    it("returns empty array when nothing matches", () => {
        expect(filterCommands(COMMANDS, "nonexistent", "ru")).toHaveLength(0)
    })

    it("trims whitespace from query", () => {
        const trimmed = filterCommands(COMMANDS, "  weather  ", "en")
        expect(trimmed).toHaveLength(1)
        expect(trimmed[0].id).toBe("weather")
    })

    it("falls back to English phrases when lang has no phrases", () => {
        const result = filterCommands(COMMANDS, "weather", "ua")
        expect(result).toHaveLength(1)
        expect(result[0].id).toBe("weather")
    })

    it("returns empty array for empty commands list", () => {
        expect(filterCommands([], "lights", "ru")).toHaveLength(0)
    })

    it("matches partial id segments", () => {
        const result = filterCommands(COMMANDS, ".on", "ru")
        expect(result).toHaveLength(1)
        expect(result[0].id).toBe("lights.on")
    })
})
