<script lang="ts">
    import { isJarvisRunning, jarvisRamUsage, jarvisCpuUsage } from "@/stores"

    $: cpuDisplay = $isJarvisRunning ? `${Math.round($jarvisCpuUsage * 10) / 10}%` : null
    $: ramDisplay = $isJarvisRunning ? `${$jarvisRamUsage} MB` : null
</script>

<div class="telemetry-grid">
    <div class="telemetry-card">
        <span class="telemetry-key">CPU</span>
        {#if cpuDisplay}
            <span class="telemetry-val">{cpuDisplay}</span>
        {:else}
            <span class="telemetry-val unavailable">—</span>
        {/if}
    </div>
    <div class="telemetry-card">
        <span class="telemetry-key">RAM</span>
        {#if ramDisplay}
            <span class="telemetry-val">{ramDisplay}</span>
        {:else}
            <span class="telemetry-val unavailable">—</span>
        {/if}
    </div>
</div>

<style lang="scss">
.telemetry-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
}

.telemetry-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border-radius: var(--r-lg);
    background: rgba(var(--white-rgb), 0.022);
    border: 1px solid rgba(var(--white-rgb), 0.048);
}

.telemetry-key {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: rgba(200,220,235,0.45);
}

.telemetry-val {
    font-size: 20px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: rgba(220,240,255,0.88);
    font-family: var(--font-mono);
    line-height: 1;

    &.unavailable {
        font-size: 9px;
        font-weight: 600;
        letter-spacing: 0.10em;
        color: rgba(150,170,190,0.28);
    }
}
</style>
