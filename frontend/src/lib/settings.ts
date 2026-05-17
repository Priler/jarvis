import { dbRead, dbWrite } from "./api"
import { DB_KEYS } from "./db-keys"
import type { IntentEngine } from "@/types"

// ── Types ────────────────────────────────────────────────────────────────────

export interface SettingsValues {
    microphone:            string
    wakeWordEngine:        string
    // sttEngine is backend-managed (read-only in Rust write_setting) — not writable from frontend
    intentEngine:          IntentEngine
    slotEngine:            string
    glinerModel:           string
    voskModel:             string
    noiseSuppression:      string
    vad:                   string
    gainNormalizerEnabled: boolean
    apiKeyPicovoice:       string
    ollamaUrl:             string
    ollamaModel:           string
}

// ── Normalization helpers (pure — exported for testing) ───────────────────────

const VALID_INTENT_ENGINES: ReadonlyArray<IntentEngine> = ["intent-classifier", "none"]

export function normalizeIntentEngine(raw: string): IntentEngine {
    return (VALID_INTENT_ENGINES as readonly string[]).includes(raw)
        ? (raw as IntentEngine)
        : "none"
}

/** Coerce raw DB string map into a validated SettingsValues. */
export function normalizeSettingsValues(raw: Record<string, string>): SettingsValues {
    return {
        microphone:            raw.microphone       ?? "",
        wakeWordEngine:        raw.wakeWordEngine    ?? "",
        intentEngine:          normalizeIntentEngine(raw.intentEngine ?? ""),
        slotEngine:            raw.slotEngine        ?? "",
        glinerModel:           raw.glinerModel       ?? "",
        voskModel:             raw.voskModel         ?? "",
        noiseSuppression:      raw.noiseSuppression  ?? "",
        vad:                   raw.vad               ?? "",
        gainNormalizerEnabled: raw.gainNormalizerEnabled === "true",
        apiKeyPicovoice:       raw.apiKeyPicovoice   ?? "",
        ollamaUrl:             raw.ollamaUrl || "http://localhost:11434",
        ollamaModel:           raw.ollamaModel       ?? "",
    }
}

// ── DB read / write ──────────────────────────────────────────────────────────

export async function loadSettingsValues(): Promise<SettingsValues> {
    const settled = await Promise.allSettled([
        dbRead(DB_KEYS.microphone),
        dbRead(DB_KEYS.wakeWordEngine),
        dbRead(DB_KEYS.intentEngine),
        dbRead(DB_KEYS.slotEngine),
        dbRead(DB_KEYS.glinerModel),
        dbRead(DB_KEYS.voskModel),
        dbRead(DB_KEYS.noiseSuppression),
        dbRead(DB_KEYS.vad),
        dbRead(DB_KEYS.gainNormalizer),
        dbRead(DB_KEYS.picovoiceApiKey),
        dbRead(DB_KEYS.ollamaUrl),
        dbRead(DB_KEYS.ollamaModel),
    ])

    const val = (i: number): string =>
        settled[i].status === 'fulfilled' ? (settled[i] as PromiseFulfilledResult<string>).value : ""

    return normalizeSettingsValues({
        microphone:            val(0),
        wakeWordEngine:        val(1),
        intentEngine:          val(2),
        slotEngine:            val(3),
        glinerModel:           val(4),
        voskModel:             val(5),
        noiseSuppression:      val(6),
        vad:                   val(7),
        gainNormalizerEnabled: val(8),
        apiKeyPicovoice:       val(9),
        ollamaUrl:             val(10),
        ollamaModel:           val(11),
    })
}

export async function saveSettingsValues(s: SettingsValues & { voiceVal: string }): Promise<void> {
    const results = await Promise.allSettled([
        dbWrite(DB_KEYS.voice,            s.voiceVal),
        dbWrite(DB_KEYS.microphone,       s.microphone),
        dbWrite(DB_KEYS.wakeWordEngine,   s.wakeWordEngine),
        dbWrite(DB_KEYS.intentEngine,     s.intentEngine),
        dbWrite(DB_KEYS.slotEngine,       s.slotEngine),
        dbWrite(DB_KEYS.glinerModel,      s.glinerModel),
        dbWrite(DB_KEYS.voskModel,        s.voskModel),
        dbWrite(DB_KEYS.noiseSuppression, s.noiseSuppression),
        dbWrite(DB_KEYS.vad,              s.vad),
        dbWrite(DB_KEYS.gainNormalizer,   s.gainNormalizerEnabled.toString()),
        dbWrite(DB_KEYS.picovoiceApiKey,  s.apiKeyPicovoice),
        dbWrite(DB_KEYS.ollamaUrl,        s.ollamaUrl),
        dbWrite(DB_KEYS.ollamaModel,      s.ollamaModel),
    ])
    const failed = results.filter(r => r.status === 'rejected' || (r.status === 'fulfilled' && r.value === false))
    if (failed.length > 0) throw new Error(`${failed.length} setting(s) rejected by backend`)
}
