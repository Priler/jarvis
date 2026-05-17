<script lang="ts">
    import { createEventDispatcher } from "svelte"
    import Select from "@/components/ui/Select.svelte"
    import Button from "@/components/ui/Button.svelte"
    import type { SelectOption } from "@/types"
    import {
        WAKE_WORD_ENGINE_OPTIONS,
        INTENT_ENGINE_OPTIONS,
        SLOT_ENGINE_OPTIONS,
        NOISE_SUPPRESSION_OPTIONS,
        VAD_OPTIONS,
    } from "@/lib/engine-options"

    export let t: (key: string) => string
    export let selectedWakeWordEngine: string
    export let selectedIntentRecognitionEngine: string
    export let selectedSlotExtractionEngine: string
    export let selectedGlinerModel: string
    export let selectedVoskModel: string
    export let selectedNoiseSuppression: string
    export let selectedVad: string
    export let gainNormalizerEnabled: boolean
    export let apiKeyPicovoice: string
    export let ollamaUrl: string
    export let ollamaModel: string
    export let availableVoskModels: SelectOption[]
    export let availableGlinerModels: SelectOption[]
    export let availableOllamaModels: SelectOption[]
    export let ollamaLoading: boolean
    export let ollamaError: string
    export let ollamaModelsLoaded: boolean

    const dispatch = createEventDispatcher<{ loadOllama: void }>()
</script>

<div class="settings-section">
    <span class="section-label">{t('settings-wake-word-engine')}</span>
    <p class="section-desc">{t('settings-wake-word-desc')}</p>
    <Select
        data={WAKE_WORD_ENGINE_OPTIONS}
        bind:value={selectedWakeWordEngine}
    />
    {#if selectedWakeWordEngine === "Picovoice"}
        <div class="warn-panel">
            <p class="warn-panel-text">{t('settings-picovoice-warning')}</p>
            <p class="warn-panel-text">{t('settings-picovoice-key-desc')} <a href="https://console.picovoice.ai/" target="_blank" rel="noopener noreferrer" class="warn-link">Picovoice Console</a>.</p>
            <label class="field-label" for="picovoice-key-input">{t('settings-picovoice-key')}</label>
            <input
                id="picovoice-key-input"
                class="field-input picovoice-key-input"
                type="password"
                placeholder={t('settings-picovoice-key')}
                autocomplete="new-password"
                maxlength="256"
                bind:value={apiKeyPicovoice}
            />
        </div>
    {/if}
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-vosk-model')}</span>
    <p class="section-desc">{t('settings-vosk-model-desc')}</p>
    <Select
        data={[{ label: t('settings-auto-detect'), value: "" }, ...availableVoskModels]}
        bind:value={selectedVoskModel}
    />
    {#if availableVoskModels.length === 0}
        <p class="info-hint">{t('settings-models-hint')}</p>
    {/if}
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-intent-engine')}</span>
    <p class="section-desc">{t('settings-intent-engine-desc')}</p>
    <Select
        data={INTENT_ENGINE_OPTIONS.map(o =>
            o.value === 'none' ? { ...o, label: t('settings-disabled') } :
            o.value === 'intent-classifier' ? { ...o, label: t('settings-intent-classifier') || o.label } :
            o
        )}
        bind:value={selectedIntentRecognitionEngine}
    />
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-slot-engine')}</span>
    <p class="section-desc">{t('settings-slot-engine-desc')}</p>
    <Select
        data={SLOT_ENGINE_OPTIONS.map(o => o.value === "None" ? { ...o, label: t('settings-disabled') } : o)}
        bind:value={selectedSlotExtractionEngine}
    />
    {#if selectedSlotExtractionEngine === "GLiNER"}
        <div class="sub-field">
            <span class="field-label">{t('settings-gliner-model')}</span>
            <p class="field-desc">{t('settings-gliner-model-desc')}</p>
            <Select
                data={[{ label: t('settings-auto-detect'), value: "" }, ...availableGlinerModels]}
                bind:value={selectedGlinerModel}
            />
            {#if availableGlinerModels.length === 0}
                <p class="info-hint">{t('settings-gliner-models-hint')}</p>
            {/if}
        </div>
    {/if}
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-noise-suppression')}</span>
    <p class="section-desc">{t('settings-noise-suppression-desc')}</p>
    <Select
        data={NOISE_SUPPRESSION_OPTIONS.map(o => o.value === "None" ? { ...o, label: t('settings-disabled') } : o)}
        bind:value={selectedNoiseSuppression}
    />
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-vad')}</span>
    <p class="section-desc">{t('settings-vad-desc')}</p>
    <Select
        data={VAD_OPTIONS.map(o => o.value === "None" ? { ...o, label: t('settings-disabled') } : o)}
        bind:value={selectedVad}
    />
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-gain-normalizer')}</span>
    <p class="section-desc">{t('settings-gain-normalizer-desc')}</p>
    <div class="toggle-row">
        <button
            type="button"
            class="toggle-btn"
            class:on={gainNormalizerEnabled}
            on:click={() => gainNormalizerEnabled = !gainNormalizerEnabled}
            aria-pressed={gainNormalizerEnabled}
            aria-label={t('settings-gain-normalizer')}
        >
            <span class="toggle-thumb"></span>
        </button>
        <span class="toggle-state">{gainNormalizerEnabled ? t('settings-enabled') : t('settings-disabled')}</span>
    </div>
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-ollama', 'OLLAMA')}</span>
    <p class="section-desc">{t('settings-ollama-desc')}</p>
    <label class="field-label" for="ollama-url-input">{t('settings-ollama-url') || 'Ollama URL'}</label>
    <div class="ollama-url-row">
        <input
            id="ollama-url-input"
            class="field-input"
            placeholder="http://localhost:11434"
            bind:value={ollamaUrl}
        />
        <Button size="sm" on:click={() => dispatch('loadOllama')} disabled={ollamaLoading}>
            {ollamaLoading ? '...' : t('settings-ollama-load-models')}
        </Button>
    </div>
    {#if ollamaError}
        <p class="error-text">{ollamaError}</p>
    {/if}
    {#if availableOllamaModels.length > 0}
        <div class="sub-field">
            <Select
                data={availableOllamaModels}
                bind:value={ollamaModel}
            />
        </div>
    {:else if ollamaModelsLoaded}
        <p class="info-hint">{t('settings-ollama-no-models')}</p>
    {/if}
</div>

<style lang="scss">
.warn-panel {
    margin-top: 12px;
    padding: 14px;
    border-radius: var(--r-lg);
    background: rgba(255,190,90,0.04);
    border: 1px solid rgba(255,190,90,0.14);
}

.warn-panel-text {
    font-size: 0.72rem;
    color: rgba(255,190,90,0.82);
    line-height: 1.5;
    margin: 0 0 4px;
}

.warn-link {
    color: rgba(255,190,90,0.9);
    text-decoration: underline;
    text-underline-offset: 2px;
}

.toggle-row {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    margin-top: 2px;
}

.toggle-state {
    font-size: 13px;
    color: rgba(210,230,245,0.78);
}

.toggle-btn {
    position: relative;
    width: 38px;
    height: 22px;
    border-radius: var(--r-xl);
    border: 1px solid rgba(255,255,255,0.12);
    background: rgba(255,255,255,0.06);
    cursor: pointer;
    transition: background 200ms ease, border-color 200ms ease;
    flex-shrink: 0;

    &.on {
        background: rgba(var(--accent-rgb), 0.22);
        border-color: rgba(var(--accent-rgb), 0.4);
    }
}

.toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: rgba(255,255,255,0.4);
    transition: transform 200ms ease, background 200ms ease;
}

.toggle-btn.on .toggle-thumb {
    transform: translateX(16px);
    background: var(--accent);
}

.ollama-url-row {
    display: flex;
    gap: 8px;
    align-items: center;
}

.field-input {
    flex: 1;
    height: 40px;
    background: rgba(255,255,255,0.025);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    color: rgba(230,245,255,0.92);
    font-family: var(--font);
    font-size: 0.82rem;
    padding: 0 14px;
    transition: border-color 140ms ease, background 140ms ease, box-shadow 140ms ease;
    outline: none;

    &::placeholder { color: var(--text-muted); opacity: 0.5; }
    &:focus {
        border-color: var(--border-focus);
        background: var(--accent-glow-xs);
        box-shadow: var(--glow-sm);
    }
}

.picovoice-key-input {
    margin-top: 10px;
    width: 100%;
}
</style>
