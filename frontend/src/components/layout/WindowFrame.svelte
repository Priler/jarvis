<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import type { UnlistenFn } from '@tauri-apps/api/event'

    let appWindow: ReturnType<typeof getCurrentWindow> | null = null
    let isMaximized = false
    let unlisten: UnlistenFn | null = null

    onMount(async () => {
        const win = getCurrentWindow()
        appWindow = win
        isMaximized = await win.isMaximized()
        unlisten = await win.onResized(async () => {
            isMaximized = await win.isMaximized()
        })
    })

    onDestroy(() => { unlisten?.() })

    function minimize()   { appWindow?.minimize() }
    function toggleMax()  { appWindow?.toggleMaximize() }
    function closeWin()   { appWindow?.close() }
</script>

<div class="win-controls">
    <button class="win-btn" on:click={minimize} title="Minimize" aria-label="Minimize">
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
            <line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
    </button>

    <button class="win-btn" on:click={toggleMax} title={isMaximized ? 'Restore' : 'Maximize'} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
        {#if isMaximized}
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
                <path d="M0 4 L0 9.5 L5.5 9.5 L5.5 7" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" stroke-linejoin="round"/>
                <rect x="3.5" y="0.5" width="6" height="6" stroke="currentColor" stroke-width="1.1"/>
            </svg>
        {:else}
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
                <rect x="1" y="1" width="8" height="8" stroke="currentColor" stroke-width="1.1"/>
            </svg>
        {/if}
    </button>

    <button class="win-btn win-btn--close" on:click={closeWin} title="Close" aria-label="Close">
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" xmlns="http://www.w3.org/2000/svg">
            <line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
    </button>
</div>

<style lang="scss">
.win-controls {
    display: flex;
    align-items: center;
    align-self: stretch;
    border-left: 1px solid rgba(var(--white-rgb), 0.04);
    padding: 0 2px;
    -webkit-app-region: no-drag;
    flex-shrink: 0;
}

.win-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 42px;
    height: 30px;
    background: transparent;
    border: none;
    border-radius: 2px;
    color: rgba(var(--white-rgb), 0.32);
    cursor: pointer;
    transition: background 140ms ease, color 140ms ease;
    -webkit-app-region: no-drag;
    flex-shrink: 0;

    &:hover {
        background: rgba(var(--white-rgb), 0.05);
        color: rgba(var(--white-rgb), 0.85);
    }

    &--close:hover {
        background: rgba(255,70,70,0.12);
        color: #fff;
    }
}
</style>
