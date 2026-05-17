<script lang="ts">
    import { onMount, onDestroy } from "svelte"
    import { Router } from "@roxi/routify"
    import routes from "../.routify/routes.default.js"
    import Events from "./Events.svelte"
    import Toasts from "@/components/ui/Toasts.svelte"

    import { disconnectIpc, stopStatsPolling } from "@/stores"
    import { stopEventTracking } from "@/lib/stores/event-tracker"
    import { criticalInit, deferredInit } from "@/lib/bootstrap"

    let ready = false

    onMount(async () => {
        await criticalInit()
        ready = true
        deferredInit()
    })

    onDestroy(() => {
        disconnectIpc()
        stopStatsPolling()
        stopEventTracking()
    })
</script>

{#if !ready}
    <div class="app-init" aria-busy="true" aria-live="polite" aria-label="Initializing">
        <div class="init-dot"></div>
    </div>
{:else}
    <Router {routes} />
    <Events />
{/if}
<Toasts />

<style>
.app-init {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: var(--bg-base);
}

.init-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.6;
    animation: init-pulse 1.2s ease-in-out infinite;
}

@keyframes init-pulse {
    0%, 100% { opacity: 0.6; transform: scale(1); }
    50%       { opacity: 1;   transform: scale(1.4); }
}

@media (prefers-reduced-motion: reduce) {
    .init-dot { animation: none; opacity: 0.8; }
}
</style>
