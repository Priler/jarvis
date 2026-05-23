// Shared domain types — used across components and stores

// ── Engine / enum-like domain values ────────────────────────────────────────

/** Intent recognition backend choices stored in the DB */
export type IntentEngine = "intent-classifier" | "none"

/** Known command types emitted by the backend. Future types remain as string. */
export const COMMAND_TYPE = {
    LUA:           "lua",
    AHK:           "ahk",
    CLI:           "cli",
    VOICE:         "voice",
    TERMINATE:     "terminate",
    STOP_CHAINING: "stop_chaining",
} as const
export type CommandType = typeof COMMAND_TYPE[keyof typeof COMMAND_TYPE]

// ── Entities ─────────────────────────────────────────────────────────────────

export interface VoiceMeta {
    id: string
    name: string
    author: string
    languages: string[]
}

export interface VoiceConfig {
    voice: VoiceMeta
}

export interface SlotDefinition {
    entity: string
    context: string[]
}

export interface JCommand {
    id: string
    type: string
    description: string
    phrases: Record<string, string[]>
    slots: Record<string, SlotDefinition>
}

export interface JarvisStats {
    running: boolean
    ram_mb: number
    cpu_usage: number
}

export interface AppInfo {
    authorName:        string
    tgOfficialLink:    string
    feedbackLink:      string
    repositoryLink:    string
    boostySupportLink: string
    patreonSupportLink: string
    logFilePath:       string
}

export interface RuntimeEvent {
    id: number
    title: string
    detail: string
    time: string
}

export interface SelectOption {
    label: string
    value: string
}

export interface VoskModel {
    name: string
    language: string
    size: string
}

export interface GlinerModel {
    display_name: string
    value: string
}
