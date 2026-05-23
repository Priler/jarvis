<script lang="ts">
    import { createEventDispatcher } from "svelte"
    import Select from "@/components/ui/Select.svelte"
    import type { SelectOption } from "@/types"

    export let t: (key: string) => string
    export let availableMicrophones: SelectOption[]
    export let selectedMicrophone: string

    const dispatch = createEventDispatcher<{ refresh: void }>()
</script>

<div class="settings-section">
    <div class="label-row">
        <span class="section-label">{t('settings-microphone')}</span>
        <button class="refresh-btn" type="button" title="Refresh devices" on:click={() => dispatch('refresh')}>↻</button>
    </div>
    <p class="section-desc">{t('settings-microphone-desc')}</p>
    <Select
        data={availableMicrophones}
        bind:value={selectedMicrophone}
    />
</div>

<style lang="scss">
.label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;

    .section-label { margin-bottom: 0; }
}

.refresh-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 1rem;
    color: var(--text-sub);
    opacity: 0.5;
    padding: 2px 6px;
    border-radius: var(--r-sm);
    transition: opacity 140ms ease, color 140ms ease;

    &:hover {
        opacity: 1;
        color: var(--accent);
    }
}
</style>
