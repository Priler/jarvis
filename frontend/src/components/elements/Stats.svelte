<script lang="ts">
    import { onMount } from "svelte"
    import { isJarvisRunning, jarvisRamUsage, tStore, audioDevices, loadAudioDevices, settingsSnapshot, loadSettingsSnapshot } from "@/stores"

    $: t = $tStore

    let microphoneName = ""

    $: neuralLabel = $settingsSnapshot.wakeWordEngine === $settingsSnapshot.sttEngine
        ? $settingsSnapshot.wakeWordEngine
        : `${$settingsSnapshot.wakeWordEngine} + ${$settingsSnapshot.sttEngine}`

    onMount(async () => {
        microphoneName = t('stats-loading')
        try {
            await Promise.all([loadAudioDevices(), loadSettingsSnapshot()])
            const { microphoneIndex } = $settingsSnapshot
            const devices = $audioDevices
            if (microphoneIndex && microphoneIndex !== "-1") {
                const idx = parseInt(microphoneIndex)
                if (!isNaN(idx) && devices[idx]) microphoneName = devices[idx]
            } else {
                microphoneName = t('stats-system-default')
            }
        } catch {
            microphoneName = t('stats-not-selected')
        }
    })

    function truncate(str: string, max: number): string {
        return str.length > max ? str.slice(0, max) + "..." : str
    }
</script>

<div class="footer-separator"></div>

<div class="footer-statusbar">

    <div class="footer-status-item">
        <svg class="footer-status-icon" width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
            <rect x="6.5" y="2" width="5" height="8.5" rx="2.5" stroke="currentColor" stroke-width="1.2"/>
            <path d="M3.5 9C3.5 12.59 5.91 15 9 15C12.09 15 14.5 12.59 14.5 9" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <line x1="9" y1="15" x2="9" y2="17" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <line x1="6" y1="17" x2="12" y2="17" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
        <div class="footer-status-text">
            <div class="footer-status-label">{t('stats-microphone')}</div>
            <div class="footer-status-value" title="{microphoneName}">{truncate(microphoneName, 18)}</div>
        </div>
    </div>

    <div class="footer-divider"></div>

    <div class="footer-status-item">
        <svg class="footer-status-icon" width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
            <circle cx="3.5" cy="6"  r="1.5" stroke="currentColor" stroke-width="1.1"/>
            <circle cx="3.5" cy="12" r="1.5" stroke="currentColor" stroke-width="1.1"/>
            <circle cx="9"   cy="4"  r="1.5" stroke="currentColor" stroke-width="1.1"/>
            <circle cx="9"   cy="9"  r="1.5" stroke="currentColor" stroke-width="1.1"/>
            <circle cx="9"   cy="14" r="1.5" stroke="currentColor" stroke-width="1.1"/>
            <circle cx="14.5" cy="6"  r="1.5" stroke="currentColor" stroke-width="1.1"/>
            <circle cx="14.5" cy="12" r="1.5" stroke="currentColor" stroke-width="1.1"/>
            <line x1="5"    y1="6"  x2="7.5" y2="4"  stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
            <line x1="5"    y1="6"  x2="7.5" y2="9"  stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
            <line x1="5"    y1="12" x2="7.5" y2="9"  stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
            <line x1="5"    y1="12" x2="7.5" y2="14" stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
            <line x1="10.5" y1="4"  x2="13"  y2="6"  stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
            <line x1="10.5" y1="9"  x2="13"  y2="6"  stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
            <line x1="10.5" y1="9"  x2="13"  y2="12" stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
            <line x1="10.5" y1="14" x2="13"  y2="12" stroke="currentColor" stroke-width="0.9" opacity="0.6"/>
        </svg>
        <div class="footer-status-text">
            <div class="footer-status-label">{t('stats-neural-networks')}</div>
            <div class="footer-status-value">{neuralLabel}</div>
        </div>
    </div>

    <div class="footer-divider"></div>

    <div class="footer-status-item">
        <svg class="footer-status-icon" width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
            <rect x="5" y="5" width="8" height="8" rx="1" stroke="currentColor" stroke-width="1.2"/>
            <rect x="7" y="7" width="4" height="4" rx="0.5" stroke="currentColor" stroke-width="1"/>
            <line x1="7.5"  y1="5"  x2="7.5"  y2="2.5"  stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <line x1="10.5" y1="5"  x2="10.5" y2="2.5"  stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <line x1="7.5"  y1="13" x2="7.5"  y2="15.5" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <line x1="10.5" y1="13" x2="10.5" y2="15.5" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <line x1="5"    y1="7.5"  x2="2.5"  y2="7.5"  stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <line x1="5"    y1="10.5" x2="2.5"  y2="10.5" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <line x1="13"   y1="7.5"  x2="15.5" y2="7.5"  stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            <line x1="13"   y1="10.5" x2="15.5" y2="10.5" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
        </svg>
        <div class="footer-status-text">
            <div class="footer-status-label">{t('stats-resources')}</div>
            <div class="footer-status-value">{$isJarvisRunning ? `RAM ${$jarvisRamUsage}MB` : '—'}</div>
        </div>
    </div>

</div>

<style lang="scss">
.footer-separator {
    height: 1px;
    width: calc(100% + 2 * var(--page-px));
    margin-left: calc(-1 * var(--page-px));
    background: var(--shell-separator);
}

.footer-statusbar {
    display: grid;
    grid-template-columns: 140px 1px 140px 1px 140px;
    column-gap: 12px;
    justify-content: center;
    align-items: center;
    margin: 0 auto;
    padding-top: 18px;
    padding-bottom: 22px;
}

.footer-status-item {
    display: flex;
    align-items: center;
    gap: 12px;
}

.footer-status-icon {
    width: 16px;
    height: 16px;
    color: var(--status-online);
    opacity: 0.88;
    filter: drop-shadow(0 0 6px var(--accent-glow-sm));
    flex-shrink: 0;
}

.footer-status-text {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
}

.footer-status-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgba(220, 235, 245, 0.92);
    line-height: 1;
}

.footer-status-value {
    margin-top: 4px;
    font-size: 10px;
    line-height: 1.3;
    color: rgba(185, 205, 220, 0.52);
}

.footer-divider {
    width: 1px;
    height: 24px;
    background: rgba(255, 255, 255, 0.06);
    justify-self: center;
    align-self: center;
}
</style>
