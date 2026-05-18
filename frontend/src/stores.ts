// IPC — connection, state stores, actions
export {
    jarvisState,
    ipcConnected,
    lastRecognizedText,
    lastExecutedCommand,
    lastError,
    pendingConfirmation,
    sandboxWarnings,
    loadingComponent,
    connectIpc,
    enableIpc,
    disableIpc,
    disconnectIpc,
    sendTextCommand,
    sendConfirmResult,
    setIpcToken,
    stopJarvisApp,
    reloadCommands
} from "./lib/ipc"

// i18n
export {
    translations,
    currentLanguage,
    tStore,
    translate,
    loadTranslations,
    setLanguage,
    getSupportedLanguages
} from "./lib/i18n"

// Runtime process state + stats polling
export {
    jarvisStats,
    isJarvisRunning,
    jarvisRamUsage,
    jarvisCpuUsage,
    updateJarvisStats,
    startStatsPolling,
    stopStatsPolling
} from "./lib/stores/runtime"

// App metadata links
export { appInfo, loadAppInfo } from "./lib/stores/app-info"

// Audio input devices
export { audioDevices, loadAudioDevices, invalidateAudioDevices } from "./lib/stores/audio"

// Runtime events log
export { runtimeEvents, addRuntimeEvent } from "./lib/stores/events"

// Assistant voice selection
export { assistantVoice, loadVoiceSetting } from "./lib/stores/voice"

// Cached display settings (microphone index, wake/STT engine) — lazy-loaded once at startup
export { settingsSnapshot, loadSettingsSnapshot, invalidateSettingsSnapshot } from "./lib/stores/settings-cache"
