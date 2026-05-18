<script lang="ts">
    import { createEventDispatcher } from "svelte"
    import { fly, fade } from "svelte/transition"
    import type { PendingConfirmation } from "@/lib/ipc/types"

    export let pending: PendingConfirmation | null = null

    const dispatch = createEventDispatcher<{ approve: void; deny: void }>()

    function handleKeydown(e: KeyboardEvent) {
        if (!pending) return
        if (e.key === "Enter") { e.preventDefault(); dispatch("approve") }
        if (e.key === "Escape") { e.preventDefault(); dispatch("deny") }
    }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if pending}
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div class="overlay-backdrop" transition:fade={{ duration: 150 }} on:click={() => dispatch("deny")}>
        <div
            class="overlay-panel"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="confirm-title"
            aria-describedby="confirm-body"
            transition:fly={{ y: 12, duration: 200 }}
            on:click|stopPropagation
        >
            <div class="confirm-icon" aria-hidden="true">⚠</div>

            <h2 class="confirm-title" id="confirm-title">Подтверждение команды</h2>

            <div class="confirm-body" id="confirm-body">
                {#if pending.description}
                    <p class="confirm-description">{pending.description}</p>
                {/if}
                <div class="confirm-cmd" aria-label="Команда">
                    <span class="cmd-label">CMD</span>
                    <code class="cmd-text">{pending.cmd}</code>
                </div>
            </div>

            <div class="confirm-actions">
                <button class="btn-deny" on:click={() => dispatch("deny")}>
                    Отмена
                </button>
                <button class="btn-approve" on:click={() => dispatch("approve")}>
                    Выполнить
                </button>
            </div>
        </div>
    </div>
{/if}

<style lang="scss">
.overlay-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(4px);
}

.overlay-panel {
    width: 340px;
    background: var(--bg-raised);
    border: 1px solid rgba(var(--color-warning-rgb), 0.28);
    border-radius: var(--r-xl);
    padding: 28px 24px 22px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    box-shadow:
        0 0 0 1px rgba(var(--color-warning-rgb), 0.08),
        0 24px 48px rgba(0, 0, 0, 0.55),
        0 0 32px rgba(var(--color-warning-rgb), 0.06);
}

.confirm-icon {
    font-size: 1.5rem;
    color: var(--color-warning);
    line-height: 1;
}

.confirm-title {
    margin: 0;
    font-size: 0.78rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--color-warning);
}

.confirm-body {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 10px;
}

.confirm-description {
    margin: 0;
    font-size: 0.72rem;
    color: var(--text-sub);
    text-align: center;
    line-height: 1.5;
}

.confirm-cmd {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--r-md);
    padding: 8px 10px;
    overflow: hidden;
}

.cmd-label {
    font-size: 0.58rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: rgba(var(--color-warning-rgb), 0.7);
    flex-shrink: 0;
}

.cmd-text {
    font-family: var(--font-mono);
    font-size: 0.68rem;
    color: rgba(255, 255, 255, 0.6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
}

.confirm-actions {
    display: flex;
    gap: 10px;
    width: 100%;
    margin-top: 4px;

    button {
        flex: 1;
        height: 36px;
        border-radius: var(--r-md);
        font-size: 0.65rem;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.1em;
        cursor: pointer;
        transition: var(--ease);
        border: 1px solid transparent;
    }
}

.btn-deny {
    background: rgba(255, 255, 255, 0.04);
    border-color: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.45);

    &:hover {
        background: rgba(255, 255, 255, 0.07);
        color: rgba(255, 255, 255, 0.7);
    }
}

.btn-approve {
    background: rgba(var(--color-warning-rgb), 0.1);
    border-color: rgba(var(--color-warning-rgb), 0.3);
    color: var(--color-warning);

    &:hover {
        background: rgba(var(--color-warning-rgb), 0.16);
        border-color: rgba(var(--color-warning-rgb), 0.45);
        box-shadow: 0 0 14px rgba(var(--color-warning-rgb), 0.12);
    }
}
</style>
