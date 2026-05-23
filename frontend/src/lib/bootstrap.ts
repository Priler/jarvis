import { loadTranslations } from "./i18n"
import { enableIpc, setIpcToken } from "./ipc"
import { loadVoiceSetting } from "./stores/voice"
import { loadAppInfo } from "./stores/app-info"
import { loadSettingsSnapshot } from "./stores/settings-cache"
import { startStatsPolling } from "./stores/runtime"
import { startEventTracking } from "./stores/event-tracker"
import { invoke } from "@tauri-apps/api/core"

/**
 * Critical init — must complete before the UI is meaningfully usable.
 * Translations are required for all user-visible text.
 * IPC is enabled here so it starts connecting as early as possible.
 */
export async function criticalInit(): Promise<void> {
    await loadTranslations()

    // Fetch per-session IPC auth token written by jarvis-app at startup.
    // If not available (jarvis-app not yet running), proceed without auth.
    try {
        const token = await invoke<string>("read_ipc_token")
        if (token) setIpcToken(token)
    } catch {
        // Token file not present — jarvis-app not started yet, connect without auth
    }

    enableIpc()
}

/**
 * Deferred init — enhances the UI but does not block rendering.
 * Failures are handled internally (toasts) and do not propagate.
 */
export function deferredInit(): void {
    loadVoiceSetting()
    loadAppInfo()
    loadSettingsSnapshot()
    startStatsPolling(5000)
    startEventTracking()
}
