<script lang="ts">
    export let t: (key: string, fallback?: string) => string
    export let wakeStatus:     string
    export let sttStatus:      string
    export let ttsStatus:      string
    export let ollamaStatus:   string
    export let pipelineStatus: string

    $: STATUS_LABEL = {
        online:    t('status-online',    'ONLINE'),
        ready:     t('status-ready',     'READY'),
        loading:   t('status-loading',   'LOADING'),
        connected: t('status-connected', 'CONNECTED'),
        active:    t('status-active',    'ACTIVE'),
        offline:   t('status-offline',   'INACTIVE'),
    } as Record<string, string>

    const STATUS_ICON: Record<string, string> = {
        online:    '●',
        ready:     '●',
        loading:   '◌',
        connected: '●',
        active:    '▶',
        offline:   '○',
    }
</script>

<div class="sys-card sys-card--primary">
    {#each [
        { name: t('system-wake-engine', 'WAKE ENGINE'), status: wakeStatus     },
        { name: t('system-stt',         'STT'),         status: sttStatus      },
        { name: t('system-tts',         'TTS'),         status: ttsStatus      },
        { name: t('system-ollama',      'OLLAMA'),      status: ollamaStatus   },
        { name: t('system-pipeline',    'VOICE PIPELINE'), status: pipelineStatus },
    ] as row}
        <div class="sys-status-row">
            <span class="sys-row-name">{row.name}</span>
            <span class="sys-row-status st-{row.status}" aria-label={STATUS_LABEL[row.status]}>
                <span class="status-icon" aria-hidden="true">{STATUS_ICON[row.status]}</span>
                {STATUS_LABEL[row.status]}
            </span>
        </div>
    {/each}
</div>

<style lang="scss">
.sys-row-name {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.10em;
    color: rgba(220,235,245,0.88);
}

.sys-row-status {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    display: flex;
    align-items: center;
    gap: 5px;

    &.st-online    { color: var(--status-online); }
    &.st-ready     { color: var(--status-ready); }
    &.st-connected { color: var(--status-connected); }
    &.st-active    { color: var(--status-active); }
    &.st-loading   { color: var(--status-loading); }
    &.st-offline   { color: var(--status-offline); }
}

.status-icon {
    font-size: 8px;
    line-height: 1;
    flex-shrink: 0;
}
</style>
