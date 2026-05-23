import { invoke } from "@tauri-apps/api/core"
import type { JarvisStats, VoiceConfig, JCommand } from "@/types"

// ── DB ─────────────────────────────────────────────────────────────────────
export const dbRead  = (key: string)               => invoke<string>("db_read",  { key })
export const dbWrite = (key: string, val: string)  => invoke<boolean>("db_write",  { key, val })

// ── App meta ────────────────────────────────────────────────────────────────
export const getAuthorName      = () => invoke<string>("get_author_name")
export const getAppVersion      = () => invoke<string>("get_app_version")
export const getTgOfficialLink  = () => invoke<string>("get_tg_official_link")
export const getFeedbackLink    = () => invoke<string>("get_feedback_link")
export const getRepositoryLink  = () => invoke<string>("get_repository_link")
export const getBoostyLink      = () => invoke<string>("get_boosty_link")
export const getPatreonLink     = () => invoke<string>("get_patreon_link")
export const getLogFilePath     = () => invoke<string>("get_log_file_path")

// ── Runtime ─────────────────────────────────────────────────────────────────
export const getJarvisStats = () => invoke<JarvisStats>("get_jarvis_app_stats")
export const runJarvisApp   = () => invoke<void>("run_jarvis_app")

// ── i18n ────────────────────────────────────────────────────────────────────
export const getTranslations      = ()           => invoke<Record<string, string>>("get_translations")
export const getCurrentLanguage   = ()           => invoke<string>("get_current_language")
export const setLanguageInvoke    = (lang: string) => invoke<Record<string, string>>("set_language", { lang })
export const getSupportedLangs    = ()           => invoke<string[]>("get_supported_languages")

// ── Audio / models ──────────────────────────────────────────────────────────
export const getAudioDevices  = () => invoke<string[]>("pv_get_audio_devices")
export const listVoices       = () => invoke<VoiceConfig[]>("list_voices")
export const listVoskModels   = () => invoke<{ name: string; language: string; size: string }[]>("list_vosk_models")
export const listGlinerModels = () => invoke<{ display_name: string; value: string }[]>("list_gliner_models")
export const listOllamaModels = (url: string) => invoke<string[]>("list_ollama_models", { url })

// ── Commands ────────────────────────────────────────────────────────────────
export const getCommandsList = () => invoke<JCommand[]>("get_commands_list")

// ── Audio playback ───────────────────────────────────────────────────────────
export const playSound    = (filename: string) => invoke<void>("play_sound", { filename, sleep: true })
export const previewVoice = (voiceId: string)  => invoke<void>("preview_voice", { voiceId })

// ── Shell ────────────────────────────────────────────────────────────────────
export const showInFolder = (path: string) => invoke<void>("show_in_folder", { path })
