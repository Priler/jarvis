<script lang="ts">
    import { createEventDispatcher } from "svelte"
    import { previewVoice } from "@/lib/api"
    import { addToast } from "@/lib/toast"
    import Select from "@/components/ui/Select.svelte"
    import type { VoiceMeta } from "@/types"

    export let t: (key: string, fallback?: string) => string
    export let languages: { code: string; name: string }[]
    export let currentLanguage: string
    export let availableVoices: VoiceMeta[]
    export let voiceVal: string

    const dispatch = createEventDispatcher<{ languageChange: string }>()

    $: selectedVoiceMeta = availableVoices.find(v => v.id === voiceVal)

    let previewLoading = false

    async function handleVoiceChange(e: CustomEvent<string>) {
        previewLoading = true
        try {
            await previewVoice(e.detail)
        } catch (err) {
            console.error("Failed to preview voice:", err)
            addToast(t('error-preview-voice') || "Failed to preview voice", "error")
        } finally {
            previewLoading = false
        }
    }
</script>

<div class="settings-section">
    <span class="section-label">{t('settings-language')}</span>
    <Select
        data={languages.map(l => ({ value: l.code, label: l.name }))}
        value={currentLanguage || "ru"}
        on:change={(e) => dispatch('languageChange', e.detail)}
    />
</div>

<div class="settings-section">
    <span class="section-label">{t('settings-voice')}</span>
    <p class="section-desc">{t('settings-voice-desc')}</p>
    {#if availableVoices.length > 0}
        <Select
            data={availableVoices.map(v => ({ value: v.id, label: v.name }))}
            bind:value={voiceVal}
            disabled={previewLoading}
            on:change={handleVoiceChange}
        />
        {#if previewLoading}
            <p class="info-hint">{t('settings-voice-previewing') || "Playing preview…"}</p>
        {/if}
        {#if selectedVoiceMeta}
            <div class="voice-meta">
                {#if selectedVoiceMeta.author}
                    <span class="voice-meta-author">by {selectedVoiceMeta.author}</span>
                {/if}
                {#if selectedVoiceMeta.languages.length > 0}
                    <span class="voice-meta-langs">{selectedVoiceMeta.languages.map(l => l.toUpperCase()).join(' · ')}</span>
                {/if}
            </div>
        {/if}
    {:else}
        <p class="empty-hint">{t('settings-no-voices')}</p>
    {/if}
</div>
