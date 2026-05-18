<script lang="ts">
    import { onDestroy } from "svelte"
    import { runJarvisApp } from "@/lib/api"
    import { addToast } from "@/lib/toast"

    import SearchBar from "@/components/elements/SearchBar.svelte"
    import ArcReactor from "@/components/elements/ArcReactor.svelte"
    import Stats from "@/components/elements/Stats.svelte"
    import { get } from "svelte/store"
    import {
        isJarvisRunning,
        updateJarvisStats,
        tStore,
        loadingComponent
    } from "@/stores"

    $: t = $tStore

    let processRunning = false
    let launching = false

    const unsubRunning = isJarvisRunning.subscribe((value) => {
        processRunning = value
    })

    onDestroy(() => {
        unsubRunning()
    })

    async function runAssistant() {
        launching = true
        try {
            await runJarvisApp()
            const poll = async (attemptsLeft: number) => {
                await updateJarvisStats()
                if (get(isJarvisRunning)) {
                    launching = false
                } else if (attemptsLeft <= 0) {
                    launching = false
                    addToast(t('error-start-timeout') || "JARVIS failed to start", "error")
                } else {
                    setTimeout(() => poll(attemptsLeft - 1), 500)
                }
            }
            setTimeout(() => poll(12), 500)
        } catch (err: unknown) {
            console.error("Failed to run jarvis-app:", err)
            addToast(t('error-start-failed') || "Failed to start JARVIS", "error")
            launching = false
        }
    }
</script>

<div class="assist-page">
    <div class="search-section">
        <SearchBar />
    </div>

    <div class="reactor-section">
        <div class="reactor-group">
            <div class="reactor-wrapper">
                <ArcReactor />
            </div>

            {#if $loadingComponent}
                <div class="loading-badge" aria-live="polite">
                    <span class="loading-spinner" aria-hidden="true"></span>
                    <span class="loading-text">{$loadingComponent}&hellip;</span>
                </div>
            {:else if !processRunning}
                <div class="offline-badge">
                    <span class="offline-icon" aria-hidden="true">⚠</span>
                    <span class="offline-text">{t('assistant-not-running')}</span>
                    <small>{t('assistant-offline-hint')}</small>
                </div>
                <button
                    class="start-button"
                    on:click={runAssistant}
                    disabled={launching}
                >
                    {launching ? t('btn-starting') : t('btn-start')}
                </button>
            {/if}
        </div>
    </div>

    <footer class="stats-footer">
        <Stats />
    </footer>
</div>

<style lang="scss">
.assist-page {
    display: flex;
    flex-direction: column;
    height: calc(100vh - var(--header-h));
}

.search-section {
    padding-top: 20px;
    padding-bottom: 4px;
    flex-shrink: 0;
}

.reactor-section {
    flex: 1;
    overflow: hidden;
    position: relative;
}

.reactor-group {
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding-bottom: 96px;
    transform: translateY(20px);
}

.reactor-wrapper {
    transition: opacity 0.5s ease, filter 0.5s ease;
}

.loading-badge {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    position: absolute;
    top: 40%;
    left: 50%;
    transform: translate(-50%, -50%);
    margin-top: -2.5rem;
    white-space: nowrap;
}

.loading-spinner {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 2px solid rgba(var(--accent-rgb), 0.25);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
}

@keyframes spin {
    to { transform: rotate(360deg); }
}

.loading-text {
    color: var(--accent);
    font-size: 0.72rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    opacity: 0.8;
}

.offline-badge {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    position: absolute;
    top: 40%;
    left: 50%;
    transform: translate(-50%, -50%);
    margin-top: -2.5rem;
    white-space: nowrap;

    small {
        display: block;
        position: absolute;
        top: 26px;
        text-align: center;
        width: 100%;
        font-size: 0.68rem;
        color: var(--text-muted);
        letter-spacing: 0.5px;
    }
}

.offline-icon {
    font-size: 1rem;
    color: var(--color-warning);
}

.offline-text {
    color: var(--color-warning);
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 3.5px;
    white-space: nowrap;
}

.start-button {
    position: absolute;
    top: 40%;
    left: 50%;
    transform: translate(-50%, -50%);
    margin-top: 2.8rem;
    height: 36px;
    padding: 0 1.8rem;
    background: linear-gradient(180deg, var(--accent-glow-lg), rgba(0,120,180,0.10));
    border: 1px solid var(--border-hover);
    border-radius: var(--r-md);
    color: var(--accent);
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 2.5px;
    cursor: pointer;
    transition: background 140ms ease, border-color 140ms ease, box-shadow 140ms ease, transform 140ms ease;
    box-shadow: 0 0 16px var(--accent-glow-sm);

    &:hover:not(:disabled) {
        background: linear-gradient(180deg, var(--accent-glow-xl), rgba(0,140,200,0.14));
        border-color: var(--border-focus);
        box-shadow: 0 0 20px var(--accent-glow-md);
        transform: translate(-50%, -50%) translateY(-1px);
    }

    &:disabled {
        opacity: 0.35;
        cursor: not-allowed;
    }
}

.stats-footer {
    flex-shrink: 0;
}
</style>
