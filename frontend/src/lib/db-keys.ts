// Centralised database key constants — prevents typos and enables refactoring

export const DB_KEYS = {
    voice:              "assistant_voice",
    language:           "language",
    microphone:         "selected_microphone",
    wakeWordEngine:     "selected_wake_word_engine",
    sttEngine:          "speech_to_text_engine",
    intentEngine:       "intent_backend",
    slotEngine:         "slots_backend",
    glinerModel:        "selected_gliner_model",
    voskModel:          "selected_vosk_model",
    noiseSuppression:   "noise_suppression",
    vad:                "vad_backend",
    gainNormalizer:     "gain_normalizer",
    picovoiceApiKey:    "api_key__picovoice",
    ollamaUrl:          "ollama_url",
    ollamaModel:        "ollama_model",
    vadEnergyThreshold: "vad_energy_threshold",
} as const

export type DbKey = typeof DB_KEYS[keyof typeof DB_KEYS]
