<script lang="ts">
    import { onMount, onDestroy } from "svelte"
    import { goto } from "@roxi/routify"
    import { listOllamaModels } from "@/lib/api"
    import { appInfo, assistantVoice, currentLanguage, setLanguage, tStore, audioDevices, loadAudioDevices, invalidateSettingsSnapshot, getSupportedLanguages } from "@/stores"
    import { addToast } from "@/lib/toast"
    import { saveSettingsValues } from "@/lib/settings"
    import { loadSettingsPageData } from "@/lib/settings-loader"
    import type { VoiceMeta, SelectOption, IntentEngine } from "@/types"

    import TabGeneral from "@/components/settings/TabGeneral.svelte"
    import TabDevices from "@/components/settings/TabDevices.svelte"
    import TabNeural from "@/components/settings/TabNeural.svelte"
    import TabAbout from "@/components/settings/TabAbout.svelte"
    import Button from "@/components/ui/Button.svelte"

    $: t = $tStore

    type SettingsTab = "general" | "devices" | "neural" | "about"
    const TAB_ORDER: SettingsTab[] = ["general", "devices", "neural", "about"]
    let activeTab: SettingsTab = "general"
    let loading = true

    let availableVoices: VoiceMeta[] = []
    let availableMicrophones: SelectOption[] = []
    let availableVoskModels: SelectOption[] = []
    let availableGlinerModels: SelectOption[] = []
    let saveButtonDisabled = false

    let voiceVal = ""
    let selectedMicrophone = ""
    let selectedWakeWordEngine = ""
    let selectedIntentRecognitionEngine: IntentEngine = "none"
    let selectedSlotExtractionEngine = ""
    let selectedGlinerModel = ""
    let selectedVoskModel = ""
    let selectedNoiseSuppression = ""
    let selectedVad = ""
    let gainNormalizerEnabled = false
    let apiKeyPicovoice = ""
    let ollamaUrl = "http://localhost:11434"
    let ollamaModel = ""
    let availableOllamaModels: SelectOption[] = []
    let ollamaLoading = false
    let ollamaError = ""
    let ollamaModelsLoaded = false

    const LANGUAGE_NAMES: Record<string, string> = {
        ru: 'Русский', en: 'English', ua: 'Українська',
        de: 'German',  fr: 'French',  es: 'Spanish',
    }
    let languages: { code: string; name: string }[] = [
        { code: "ru", name: "Русский" },
        { code: "en", name: "English" },
        { code: "ua", name: "Українська" },
    ]

    async function selectLanguage(code: string) {
        await setLanguage(code)
    }

    function handleTabNav(e: KeyboardEvent) {
        const idx = TAB_ORDER.indexOf(activeTab)
        if (e.key === 'ArrowRight') {
            e.preventDefault()
            activeTab = TAB_ORDER[(idx + 1) % TAB_ORDER.length]
        } else if (e.key === 'ArrowLeft') {
            e.preventDefault()
            activeTab = TAB_ORDER[(idx - 1 + TAB_ORDER.length) % TAB_ORDER.length]
        }
    }

    async function loadOllamaModels() {
        ollamaLoading = true
        ollamaError = ""
        ollamaModelsLoaded = false
        try {
            const models = await listOllamaModels(ollamaUrl)
            availableOllamaModels = models.map(m => ({ label: m, value: m }))
            ollamaModelsLoaded = true
            if (models.length > 0 && !ollamaModel) {
                ollamaModel = models[0]
            }
        } catch (err: unknown) {
            ollamaError = err instanceof Error ? err.message : t('settings-ollama-error')
            availableOllamaModels = []
        } finally {
            ollamaLoading = false
        }
    }

    const unsubVoice = assistantVoice.subscribe(value => { voiceVal = value })

    let feedbackLink = ""
    let logFilePath = ""
    let tgLink = ""
    let repoLink = ""
    let boostyLink = ""
    let patreonLink = ""
    let authorName = ""
    let appVersion = ""
    const unsubAppInfo = appInfo.subscribe(info => {
        authorName   = info.authorName
        feedbackLink = info.feedbackLink
        logFilePath  = info.logFilePath
        tgLink       = info.tgOfficialLink
        repoLink     = info.repositoryLink
        boostyLink   = info.boostySupportLink
        patreonLink  = info.patreonSupportLink
    })

    async function saveSettings() {
        saveButtonDisabled = true
        try {
            await saveSettingsValues({
                voiceVal,
                microphone:            selectedMicrophone,
                wakeWordEngine:        selectedWakeWordEngine,
                intentEngine:          selectedIntentRecognitionEngine,
                slotEngine:            selectedSlotExtractionEngine,
                glinerModel:           selectedGlinerModel,
                voskModel:             selectedVoskModel,
                noiseSuppression:      selectedNoiseSuppression,
                vad:                   selectedVad,
                gainNormalizerEnabled,
                apiKeyPicovoice,
                ollamaUrl,
                ollamaModel,
            })
            assistantVoice.set(voiceVal)
            invalidateSettingsSnapshot()
            addToast(t('notification-saved') || "Settings saved", "success")
        } catch (err: unknown) {
            console.error("failed to save settings:", err)
            addToast(t('notification-error') || "Failed to save settings", "error")
        }
        setTimeout(() => { saveButtonDisabled = false }, 1000)
    }

    onDestroy(() => {
        unsubVoice()
        unsubAppInfo()
    })

    onMount(async () => {
        try {
            const codes = await getSupportedLanguages()
            languages = codes.map(code => ({ code, name: LANGUAGE_NAMES[code] ?? code }))
        } catch { /* keep fallback defaults */ }

        try {
            await loadAudioDevices()
        } catch {
            addToast(t('error-load-audio') || "Failed to load audio devices", "error")
        }

        try {
            const data = await loadSettingsPageData()

            appVersion            = data.appVersion
            availableVoices       = data.availableVoices
            availableVoskModels   = data.availableVoskModels
            availableGlinerModels = data.availableGlinerModels

            availableMicrophones = [
                { label: t('settings-mic-default'), value: "-1" },
                ...$audioDevices.map((name, idx) => ({ label: name, value: String(idx) }))
            ]

            if (data.errors.meta)     addToast(t('error-load-meta')    || "Failed to load app metadata",  "info")
            if (data.errors.vosk)     addToast(t('error-load-vosk')    || "Failed to load Vosk models",   "info")
            if (data.errors.gliner)   addToast(t('error-load-gliner')  || "Failed to load GLiNER models", "info")
            if (data.errors.voices)   addToast(t('error-load-voices')  || "Failed to load voices",        "error")
            if (data.errors.settings) addToast(t('error-load-settings')|| "Failed to load settings",      "error")

            if (data.settings) {
                const s = data.settings
                selectedMicrophone              = s.microphone
                selectedWakeWordEngine          = s.wakeWordEngine
                selectedIntentRecognitionEngine = s.intentEngine
                selectedSlotExtractionEngine    = s.slotEngine
                selectedGlinerModel             = s.glinerModel
                selectedVoskModel               = s.voskModel
                selectedNoiseSuppression        = s.noiseSuppression
                selectedVad                     = s.vad
                gainNormalizerEnabled           = s.gainNormalizerEnabled
                apiKeyPicovoice                 = s.apiKeyPicovoice
                ollamaUrl                       = s.ollamaUrl
                ollamaModel                     = s.ollamaModel
            }
        } finally {
            loading = false
        }
    })
</script>

{#if loading}
    <div class="settings-loading" aria-busy="true">
        <div class="settings-skeleton"></div>
        <div class="settings-skeleton settings-skeleton--short"></div>
        <div class="settings-skeleton"></div>
        <div class="settings-skeleton settings-skeleton--short"></div>
    </div>
{:else}
<div class="settings-layout">
    <div class="settings-nav" role="tablist" tabindex="-1" on:keydown={handleTabNav}>
        <button class="nav-item" role="tab" id="tab-general" aria-selected={activeTab === 'general'} aria-controls="panel-settings" class:active={activeTab === 'general'} on:click={() => activeTab = 'general'}>
            {t('settings-general')}
        </button>
        <button class="nav-item" role="tab" id="tab-devices" aria-selected={activeTab === 'devices'} aria-controls="panel-settings" class:active={activeTab === 'devices'} on:click={() => activeTab = 'devices'}>
            {t('settings-devices')}
        </button>
        <button class="nav-item" role="tab" id="tab-neural" aria-selected={activeTab === 'neural'} aria-controls="panel-settings" class:active={activeTab === 'neural'} on:click={() => activeTab = 'neural'}>
            {t('settings-neural-networks')}
        </button>
        <button class="nav-item" role="tab" id="tab-about" aria-selected={activeTab === 'about'} aria-controls="panel-settings" class:active={activeTab === 'about'} on:click={() => activeTab = 'about'}>
            {t('settings-about')}
        </button>
    </div>

    <div class="settings-content"
         id="panel-settings"
         role="tabpanel"
         aria-labelledby="tab-{activeTab}">
        {#if activeTab === 'general'}
            <TabGeneral
                {t}
                {languages}
                currentLanguage={$currentLanguage || "ru"}
                {availableVoices}
                bind:voiceVal
                on:languageChange={(e) => selectLanguage(e.detail)}
            />
        {:else if activeTab === 'devices'}
            <TabDevices
                {t}
                {availableMicrophones}
                bind:selectedMicrophone
            />
        {:else if activeTab === 'neural'}
            <TabNeural
                {t}
                bind:selectedWakeWordEngine
                bind:selectedIntentRecognitionEngine
                bind:selectedSlotExtractionEngine
                bind:selectedGlinerModel
                bind:selectedVoskModel
                bind:selectedNoiseSuppression
                bind:selectedVad
                bind:gainNormalizerEnabled
                bind:apiKeyPicovoice
                bind:ollamaUrl
                bind:ollamaModel
                {availableVoskModels}
                {availableGlinerModels}
                {availableOllamaModels}
                {ollamaLoading}
                {ollamaError}
                {ollamaModelsLoaded}
                on:loadOllama={loadOllamaModels}
            />
        {:else if activeTab === 'about'}
            <TabAbout
                {t}
                {appVersion}
                {authorName}
                {feedbackLink}
                {logFilePath}
                {tgLink}
                {repoLink}
                {boostyLink}
                {patreonLink}
                currentLanguage={$currentLanguage || "en"}
            />
        {/if}
    </div>

    <div class="settings-actions">
        <Button variant="primary" on:click={saveSettings} disabled={saveButtonDisabled}>
            {t('settings-save')}
        </Button>
        <Button variant="ghost" on:click={() => $goto("/")}>
            {t('settings-back')}
        </Button>
    </div>
</div>
{/if}

<style lang="scss">
.settings-loading {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 16px;
}

.settings-skeleton {
    height: 80px;
    border-radius: var(--r-xl);
    background: linear-gradient(90deg,
        rgba(255,255,255,0.03) 0%,
        rgba(255,255,255,0.06) 50%,
        rgba(255,255,255,0.03) 100%
    );
    background-size: 200% 100%;
    animation: skeleton-sweep 1.6s ease-in-out infinite;

    &--short { height: 56px; }
}

@keyframes skeleton-sweep {
    0%   { background-position: 200% 0; }
    100% { background-position: -200% 0; }
}

.settings-layout {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding-top: 16px;
    height: calc(100vh - var(--header-h));
    overflow: hidden;
}

.settings-nav {
    display: flex;
    flex-direction: row;
    flex-shrink: 0;
    height: 38px;
    border-radius: var(--r-lg);
    overflow: hidden;
    background: rgba(255,255,255,0.02);
    border: 1px solid rgba(255,255,255,0.05);
    position: relative;
    margin-bottom: 12px;
}

.nav-item {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    height: 100%;
    padding: 0 16px;
    background: transparent;
    border: none;
    border-right: 1px solid rgba(255,255,255,0.04);
    color: rgba(255,255,255,0.35);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.10em;
    cursor: pointer;
    transition: var(--ease);
    white-space: nowrap;

    &:last-child { border-right: none; }

    &:hover {
        background: rgba(var(--accent-rgb),0.035);
        color: rgba(255,255,255,0.65);
    }

    &.active {
        background: rgba(var(--accent-rgb),0.06);
        border-right-color: rgba(var(--accent-rgb),0.12);
        color: var(--accent);
        box-shadow: 0 0 12px rgba(var(--accent-rgb),0.05);
    }
}

.settings-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 12px;
    padding-bottom: 8px;
}

.settings-actions {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px 0 24px;
    flex-shrink: 0;
    border-top: 1px solid rgba(255,255,255,0.05);

    & > :global(*) { width: 100%; }
}
</style>