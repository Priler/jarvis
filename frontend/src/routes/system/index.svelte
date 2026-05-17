<script lang="ts">
    import { onMount } from "svelte"
    import { isJarvisRunning, ipcConnected, tStore, settingsSnapshot } from "@/stores"
    import { ENGINE_DEFAULTS } from "@/lib/engine-options"
    import { loadSettingsValues } from "@/lib/settings"
    import SysOverview  from "@/components/system/SysOverview.svelte"
    import SysPipeline  from "@/components/system/SysPipeline.svelte"
    import SysTelemetry from "@/components/system/SysTelemetry.svelte"
    import SysEvents    from "@/components/system/SysEvents.svelte"
    import SysModels    from "@/components/system/SysModels.svelte"

    $: t = $tStore

    // Wake/STT engines come from the cached snapshot (already loaded in deferredInit)
    $: wakeEngine = $settingsSnapshot.wakeWordEngine || ENGINE_DEFAULTS.wakeWordEngine
    $: sttEngine  = $settingsSnapshot.sttEngine      || ENGINE_DEFAULTS.sttEngine

    let sttModel     = ""
    let intentEngine = ""
    let llmModel     = ""

    $: intentDisplay = (!intentEngine || intentEngine === 'none')
        ? t('system-not-configured', 'NOT CONFIGURED')
        : intentEngine === 'intent-classifier'
        ? t('system-intent-classifier', 'Intent Classifier')
        : intentEngine

    // ── Status derivation ─────────────────────────────────────────────────────
    $: wakeStatus     = $isJarvisRunning ? 'online'    : 'offline'
    $: sttStatus      = $ipcConnected    ? 'ready'     : ($isJarvisRunning ? 'loading' : 'offline')
    $: ttsStatus      = $ipcConnected    ? 'ready'     : ($isJarvisRunning ? 'loading' : 'offline')
    $: ollamaStatus   = llmModel         ? 'connected' : 'offline'
    $: pipelineStatus = ($isJarvisRunning && $ipcConnected) ? 'active' : ($isJarvisRunning ? 'loading' : 'offline')

    onMount(async () => {
        const s = await loadSettingsValues()
        sttModel     = s.voskModel    || t('settings-auto-detect', 'Auto-detect')
        intentEngine = s.intentEngine
        llmModel     = s.ollamaModel
    })
</script>

<div class="system-layout">
    <div class="system-content">

        <div class="sys-section sys-section--primary">
            <span class="sys-section-label">{t('system-overview', 'OVERVIEW')}</span>
            <SysOverview
                {t}
                {wakeStatus}
                {sttStatus}
                {ttsStatus}
                {ollamaStatus}
                {pipelineStatus}
            />
        </div>

        <div class="sys-section">
            <span class="sys-section-label">{t('system-pipeline', 'VOICE PIPELINE')}</span>
            <SysPipeline {t} />
        </div>

        <div class="sys-section">
            <span class="sys-section-label">{t('system-telemetry', 'TELEMETRY')}</span>
            <SysTelemetry />
        </div>

        <div class="sys-section">
            <span class="sys-section-label">{t('system-events', 'EVENTS')}</span>
            <SysEvents {t} />
        </div>

        <div class="sys-section">
            <span class="sys-section-label">{t('system-models', 'MODELS')}</span>
            <SysModels
                {t}
                {wakeEngine}
                {sttEngine}
                {sttModel}
                {intentDisplay}
                {llmModel}
            />
        </div>

    </div>
</div>

<style lang="scss">
.system-layout {
    display: flex;
    flex-direction: column;
    padding-top: 16px;
    height: calc(100vh - var(--header-h));
    overflow: hidden;
}

.system-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 12px;
    padding-bottom: 8px;
}

.sys-section {
    margin-bottom: 14px;
}

.sys-section-label {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: rgba(180,200,220,0.48);
    margin-bottom: 8px;

    &::before {
        content: '';
        flex-shrink: 0;
        width: 2px;
        height: 16px;
        background: var(--accent);
        border-radius: 2px;
        box-shadow: 0 0 8px rgba(0,229,255,0.35);
    }
}

.sys-section--primary .sys-section-label {
    color: rgba(220,235,245,0.78);
}
</style>
