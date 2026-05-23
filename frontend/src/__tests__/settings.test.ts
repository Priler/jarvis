import { describe, it, expect } from "vitest"
import { normalizeIntentEngine, normalizeSettingsValues } from "@/lib/settings"

describe("normalizeIntentEngine", () => {
    it("passes through valid intent-classifier", () => {
        expect(normalizeIntentEngine("intent-classifier")).toBe("intent-classifier")
    })

    it("passes through valid none", () => {
        expect(normalizeIntentEngine("none")).toBe("none")
    })

    it("falls back to none for unknown value", () => {
        expect(normalizeIntentEngine("")).toBe("none")
        expect(normalizeIntentEngine("llm")).toBe("none")
        expect(normalizeIntentEngine("INTENT-CLASSIFIER")).toBe("none")
        expect(normalizeIntentEngine("   ")).toBe("none")
    })
})

describe("normalizeSettingsValues", () => {
    it("returns defaults for empty input", () => {
        const result = normalizeSettingsValues({})
        expect(result.intentEngine).toBe("none")
        expect(result.ollamaUrl).toBe("http://localhost:11434")
        expect(result.gainNormalizerEnabled).toBe(false)
        expect(result.microphone).toBe("")
    })

    it("keeps valid intentEngine", () => {
        const result = normalizeSettingsValues({ intentEngine: "intent-classifier" })
        expect(result.intentEngine).toBe("intent-classifier")
    })

    it("falls back intentEngine to none when invalid", () => {
        const result = normalizeSettingsValues({ intentEngine: "unknown-engine" })
        expect(result.intentEngine).toBe("none")
    })

    it("parses gainNormalizerEnabled from string", () => {
        expect(normalizeSettingsValues({ gainNormalizerEnabled: "true" }).gainNormalizerEnabled).toBe(true)
        expect(normalizeSettingsValues({ gainNormalizerEnabled: "false" }).gainNormalizerEnabled).toBe(false)
        expect(normalizeSettingsValues({ gainNormalizerEnabled: "" }).gainNormalizerEnabled).toBe(false)
    })

    it("uses default ollamaUrl when field is empty", () => {
        expect(normalizeSettingsValues({ ollamaUrl: "" }).ollamaUrl).toBe("http://localhost:11434")
        expect(normalizeSettingsValues({ ollamaUrl: "http://custom:1234" }).ollamaUrl).toBe("http://custom:1234")
    })

    it("preserves all provided string fields", () => {
        const result = normalizeSettingsValues({
            microphone:        "2",
            wakeWordEngine:    "rustpotter",
            voskModel:         "vosk-model-ru",
            noiseSuppression:  "rnnoise",
            vad:               "silero",
            apiKeyPicovoice:   "pk-test",
            ollamaModel:       "llama3",
        })
        expect(result.microphone).toBe("2")
        expect(result.wakeWordEngine).toBe("rustpotter")
        expect(result.voskModel).toBe("vosk-model-ru")
        expect(result.noiseSuppression).toBe("rnnoise")
        expect(result.vad).toBe("silero")
        expect(result.apiKeyPicovoice).toBe("pk-test")
        expect(result.ollamaModel).toBe("llama3")
    })
})
