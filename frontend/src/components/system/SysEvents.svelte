<script lang="ts">
    import { runtimeEvents } from "@/stores"

    export let t: (key: string, fallback?: string) => string

    $: events = $runtimeEvents
</script>

{#if events.length === 0}
    <div class="sys-empty">
        <span class="sys-empty-icon">◌</span>
        <span class="sys-empty-title">{t('system-no-events', 'NO RUNTIME EVENTS')}</span>
        <span class="sys-empty-hint">{t('system-no-events-hint', 'Start JARVIS to see activity')}</span>
    </div>
{:else}
    <div class="events-list">
        {#each events as ev (ev.id)}
            <div class="event-card">
                <span class="event-title">{ev.title}</span>
                {#if ev.detail}
                    <span class="event-detail">{ev.detail}</span>
                {/if}
                <span class="event-time">{ev.time}</span>
            </div>
        {/each}
    </div>
{/if}

<style lang="scss">
.sys-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 72px;
    padding: 12px 0;
}

.sys-empty-icon {
    font-size: 18px;
    color: rgba(150,170,190,0.25);
    line-height: 1;
}

.sys-empty-title {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: rgba(180,200,220,0.38);
}

.sys-empty-hint {
    font-size: 10px;
    color: rgba(150,170,190,0.28);
}

.events-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
}

.event-card {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px 14px;
    border-radius: var(--r-lg);
    background: rgba(255,255,255,0.018);
    border: 1px solid rgba(255,255,255,0.04);
    transition: var(--ease);

    &:hover { background: rgba(0,229,255,0.018); border-color: rgba(0,229,255,0.07); }
}

.event-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgba(220,235,245,0.82);
}

.event-detail {
    font-size: 10px;
    color: rgba(160,180,200,0.55);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.event-time {
    font-size: 9px;
    color: rgba(150,170,185,0.32);
    font-family: var(--font-mono);
    letter-spacing: 0.06em;
    margin-top: 2px;
}
</style>
