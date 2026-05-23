<script lang="ts">
    import { onMount, onDestroy } from "svelte"
    import { getCommandsList } from "@/lib/api"

    import { currentLanguage, tStore, reloadCommands, ipcConnected } from "@/stores"
    import { addToast } from "@/lib/toast"
    import { filterCommands, getPhrases } from "@/lib/commands-filter"
    import type { JCommand } from "@/types"

    $: t = $tStore

    let commands: JCommand[] = []
    let searchQuery = ""
    let debouncedQuery = ""
    let _debounceTimer: ReturnType<typeof setTimeout> | undefined
    let loading = true
    let loadError = false

    $: lang = $currentLanguage || "ru"
    $: isConnected = $ipcConnected
    $: reloadTitle = isConnected
        ? "Reload backend commands & refresh list"
        : "Refresh list (JARVIS not connected — backend reload skipped)"

    function handleSearch(e: Event) {
        searchQuery = (e.target as HTMLInputElement).value
        clearTimeout(_debounceTimer)
        _debounceTimer = setTimeout(() => { debouncedQuery = searchQuery }, 150)
    }

    $: filtered = filterCommands(commands, debouncedQuery, lang)

    async function loadCommands() {
        loading = true
        loadError = false
        try {
            commands = await getCommandsList()
        } catch (err: unknown) {
            console.error("Failed to load commands:", err)
            loadError = true
        } finally {
            loading = false
        }
    }

    function handleReload() {
        if (isConnected) {
            reloadCommands()
        } else {
            addToast("JARVIS not connected — refreshing list only", "info")
        }
        loadCommands()
    }

    onMount(loadCommands)
    onDestroy(() => clearTimeout(_debounceTimer))
</script>

<div class="commands-header">
    <input
        class="shell-input"
        type="text"
        placeholder={t('commands-search')}
        value={searchQuery}
        on:input={handleSearch}
    />
    <button class="reload-btn" on:click={handleReload} title={reloadTitle} aria-label="Reload commands">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path d="M12 7A5 5 0 1 1 7 2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
            <polyline points="7,1 9,3 7,5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
    </button>
</div>

{#if loading}
    <div class="empty-state">{t('stats-loading')}</div>
{:else if loadError}
    <div class="empty-state error-state">
        <span>{t('commands-load-error')}</span>
        <button class="retry-btn" on:click={loadCommands}>{t('commands-retry')}</button>
    </div>
{:else if commands.length === 0}
    <div class="empty-state">{t('commands-no-commands')}</div>
{:else if filtered.length === 0}
    <div class="empty-state">{t('error-not-found')}</div>
{:else}
    <div class="commands-list">
        {#each filtered as cmd (cmd.id)}
            {@const phrases = getPhrases(cmd, lang)}
            <div class="command-card">
                <div class="card-header">
                    <span class="cmd-id">{cmd.id}</span>
                    <span class="cmd-type-badge" data-type={cmd.type.toLowerCase()}>
                        {cmd.type}
                    </span>
                </div>

                {#if cmd.description}
                    <p class="cmd-description">{cmd.description}</p>
                {/if}

                {#if phrases.length > 0}
                    <div class="cmd-phrases">
                        {#each phrases as phrase}
                            <span class="phrase-chip">{phrase}</span>
                        {/each}
                    </div>
                {/if}

                {#if Object.keys(cmd.slots).length > 0}
                    <div class="cmd-slots">
                        {#each Object.entries(cmd.slots) as [name, slot]}
                            <span class="slot-chip" title={slot.context.join(', ')}>
                                ❰{name}: {slot.entity}❱
                            </span>
                        {/each}
                    </div>
                {/if}
            </div>
        {/each}
    </div>
{/if}

<style lang="scss">
.commands-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding-right: 12px;
    margin-top: 16px;
    margin-bottom: 10px;
}

.reload-btn {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    background: transparent;
    border: 1px solid rgba(var(--white-rgb), 0.07);
    border-radius: var(--r-md);
    color: rgba(var(--white-rgb), 0.35);
    cursor: pointer;
    transition: var(--ease);

    &:hover {
        background: rgba(var(--accent-rgb),0.06);
        border-color: rgba(var(--accent-rgb),0.2);
        color: var(--accent);
    }
}

.empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.82rem;
    padding: 2.5rem 0;
    font-style: italic;

    &.error-state {
        color: rgba(var(--color-error-rgb), 0.75);
        font-style: normal;
        gap: 12px;
        flex-direction: column;
    }
}

.retry-btn {
    margin-top: 4px;
    padding: 6px 16px;
    background: transparent;
    border: 1px solid rgba(var(--color-error-rgb), 0.4);
    border-radius: 6px;
    color: rgba(var(--color-error-rgb), 0.75);
    font-family: var(--font);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    cursor: pointer;
    transition: border-color 140ms ease, color 140ms ease;

    &:hover {
        border-color: rgba(var(--color-error-rgb), 0.7);
        color: rgba(var(--color-error-rgb), 1);
    }
}

.commands-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-right: 12px;
    padding-bottom: 1rem;
    overflow-y: auto;
    /* 80px = commands-header height (30px input) + margins (16+10) + commands-header gap overhead */
    max-height: calc(100vh - var(--header-h) - 80px);
}

.command-card {
    background: rgba(var(--white-rgb), 0.018);
    border: 1px solid rgba(var(--white-rgb), 0.045);
    border-radius: var(--r-md);
    padding: 10px 14px;
    transition: var(--ease);

    &:hover {
        background: rgba(var(--accent-rgb), 0.018);
        border-color: rgba(var(--accent-rgb), 0.08);
    }
}

.card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 5px;
}

.cmd-id {
    font-size: 0.78rem;
    font-weight: 600;
    color: rgba(var(--white-rgb), 0.75);
    font-family: var(--font-mono);
    letter-spacing: 0.3px;
}

.cmd-type-badge {
    --type-color: #6b7280;
    font-size: 0.6rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    color: var(--type-color);
    border: 1px solid var(--type-color);
    border-radius: var(--r-sm);
    padding: 1px 5px;
    opacity: 0.82;

    &[data-type="lua"]           { --type-color: #00FFC6; }
    &[data-type="ahk"]           { --type-color: #3b82f6; }
    &[data-type="cli"]           { --type-color: #f97316; }
    &[data-type="voice"]         { --type-color: #a855f7; }
    &[data-type="terminate"]     { --type-color: #ef4444; }
    &[data-type="stop_chaining"] { --type-color: #6b7280; }
}

.cmd-description {
    font-size: 0.72rem;
    color: var(--text-muted);
    margin: 0 0 5px;
    font-style: italic;
}

.cmd-phrases {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 3px;
}

.phrase-chip {
    font-size: 0.67rem;
    background: rgba(var(--white-rgb), 0.03);
    border: 1px solid rgba(var(--white-rgb), 0.06);
    border-radius: var(--r-sm);
    padding: 2px 7px;
    color: rgba(var(--white-rgb), 0.38);
    font-style: italic;
    opacity: 0.78;
}

.cmd-slots {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 5px;
}

.slot-chip {
    font-size: 0.65rem;
    background: rgba(168,85,247,0.045);
    border: 1px solid rgba(168,85,247,0.14);
    border-radius: var(--r-sm);
    padding: 1px 6px;
    color: rgba(168,85,247,0.72);
    font-family: var(--font-mono);
    cursor: help;
    opacity: 0.78;
}
</style>
