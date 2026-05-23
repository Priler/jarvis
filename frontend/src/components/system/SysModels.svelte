<script lang="ts">
    export let t: (key: string, fallback?: string) => string
    export let wakeEngine:    string
    export let sttEngine:     string
    export let sttModel:      string
    export let intentDisplay: string
    export let llmModel:      string
</script>

<div class="sys-card">
    {#each [
        { name: t('system-wake-engine', 'WAKE ENGINE'), value: wakeEngine    || '—' },
        { name: t('system-stt-engine',  'STT ENGINE'),  value: sttEngine     || '—' },
        { name: t('system-stt-model',   'STT MODEL'),   value: sttModel      || '—' },
        { name: t('system-intent',      'INTENT'),      value: intentDisplay        },
        { name: t('system-llm',         'LLM'),         value: llmModel || t('system-not-configured', 'NOT CONFIGURED') },
    ] as row}
        <div class="sys-status-row sys-status-row--sm">
            <span class="sys-row-name">{row.name}</span>
            <span class="sys-row-value">{row.value}</span>
        </div>
    {/each}
</div>

<style lang="scss">
.sys-row-name {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.10em;
    color: rgba(210,230,245,0.58);
}

.sys-row-value {
    font-size: 10px;
    color: rgba(180,200,220,0.58);
    font-family: var(--font-mono);
    text-align: right;
    max-width: 58%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
</style>
