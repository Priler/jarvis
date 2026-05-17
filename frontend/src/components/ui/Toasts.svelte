<script lang="ts">
    import { fly } from "svelte/transition"
    import { toasts, removeToast } from "@/lib/toast"
    import { tStore } from "@/stores"

    $: t = $tStore

    const reducedMotion = typeof window !== 'undefined'
        ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
        : false
</script>

<div class="hud-toasts" aria-live="assertive" aria-atomic="false">
    {#each $toasts as toast (toast.id)}
        <div
            class="hud-toast hud-toast--{toast.type}"
            role="alert"
            in:fly={reducedMotion ? { duration: 0 } : { y: -10, duration: 220, opacity: 0 }}
            out:fly={reducedMotion ? { duration: 0 } : { y: -8,  duration: 160, opacity: 0 }}
        >
            <span class="hud-msg">{toast.message}</span>
            <button
                class="hud-close"
                aria-label={t('ui-dismiss', 'Dismiss')}
                on:click={() => removeToast(toast.id)}
            >✕</button>
        </div>
    {/each}
</div>

<style lang="scss">
.hud-toasts {
    position: fixed;
    top: 20px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 9999;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    pointer-events: none;
}

.hud-toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 10px 0 14px;
    min-height: 34px;
    min-width: 200px;
    max-width: 340px;

    background: rgba(5, 9, 15, 0.90);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255,255,255,0.055);
    border-left: 2px solid var(--hud-accent);
    border-radius: 4px;

    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.03em;
    white-space: nowrap;

    box-shadow:
        0 0 24px rgba(0,0,0,0.65),
        0 0 0 1px rgba(0,0,0,0.5),
        0 0 14px var(--hud-glow);

    pointer-events: auto;

    &--error {
        --hud-accent: rgba(255, 62, 62, 0.80);
        --hud-glow:   rgba(255, 62, 62, 0.07);
        color: rgba(255, 145, 145, 0.92);
    }

    &--success {
        --hud-accent: rgba(0, 215, 120, 0.78);
        --hud-glow:   rgba(0, 215, 120, 0.07);
        color: rgba(80, 215, 158, 0.92);
    }

    &--info {
        --hud-accent: rgba(0, 229, 255, 0.75);
        --hud-glow:   rgba(0, 229, 255, 0.07);
        color: rgba(0, 229, 255, 0.88);
    }
}

.hud-msg {
    flex: 1;
    padding: 8px 0;
    line-height: 1.35;
    overflow: hidden;
    text-overflow: ellipsis;
}

.hud-close {
    flex-shrink: 0;
    background: transparent;
    border: none;
    color: inherit;
    opacity: 0.38;
    cursor: pointer;
    padding: 4px 2px;
    font-size: 10px;
    line-height: 1;
    pointer-events: auto;
    transition: opacity 100ms ease;

    &:hover { opacity: 0.85; }
}
</style>
