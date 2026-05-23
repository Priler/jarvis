import type { SelectOption } from "@/types"

export const WAKE_WORD_ENGINE_OPTIONS: SelectOption[] = [
    { label: "Rustpotter",           value: "Rustpotter" },
    { label: "Vosk",                 value: "Vosk"       },
    { label: "Picovoice Porcupine",  value: "Picovoice"  },
]

export const INTENT_ENGINE_OPTIONS: SelectOption[] = [
    { label: "Intent Classifier", value: "intent-classifier" },
    { label: "Disabled",          value: "none"              },
]

export const SLOT_ENGINE_OPTIONS: SelectOption[] = [
    { label: "Disabled",  value: "None"   },
    { label: "GLiNER (NER)", value: "GLiNER" },
]

export const NOISE_SUPPRESSION_OPTIONS: SelectOption[] = [
    { label: "Disabled",    value: "None"        },
    { label: "Nnnoiseless", value: "Nnnoiseless" },
]

export const VAD_OPTIONS: SelectOption[] = [
    { label: "Disabled",    value: "None"        },
    { label: "Energy",      value: "Energy"      },
    { label: "Nnnoiseless", value: "Nnnoiseless" },
]

export const DEFAULT_VOICE_ID = "jarvis-og"

export const LANGUAGE_NAMES: Record<string, string> = {
    ru: 'Русский', en: 'English', ua: 'Українська',
    de: 'German',  fr: 'French',  es: 'Spanish',
}

export const ENGINE_DEFAULTS = {
    wakeWordEngine:   "Rustpotter",
    sttEngine:        "Vosk",
    intentEngine:     "none",
    slotEngine:       "None",
    noiseSuppression: "None",
    vad:              "None",
} as const
