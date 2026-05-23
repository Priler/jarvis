<script lang="ts">
    import { tStore, isJarvisRunning, ipcConnected, sendTextCommand } from "@/stores"
    import { addToast } from "@/lib/toast"

    $: t = $tStore

    let searchQuery = ""
    let isProcessing = false

    function handleSubmit(e: Event) {
        e.preventDefault()

        const command = searchQuery.trim()
        if (!command || isProcessing) return

        if (!$isJarvisRunning || !$ipcConnected) {
            addToast(t('search-error-not-running'), "error")
            return
        }

        isProcessing = true

        try {
            const sent = sendTextCommand(command)
            if (!sent) {
                addToast(t('search-error-not-running'), "info")
            } else {
                searchQuery = ""
            }
        } catch (err: unknown) {
            console.error("Failed to send command:", err)
            addToast(t('search-error-failed'), "error")
        } finally {
            isProcessing = false
        }
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Escape") {
            searchQuery = ""
        }
    }
</script>

<div id="search-form" class="search" class:active={searchQuery !== ""} class:processing={isProcessing}>
    <form on:submit={handleSubmit} aria-busy={isProcessing}>
        <input
            bind:value={searchQuery}
            on:keydown={handleKeydown}
            type="text"
            name="q"
            placeholder={t('search-placeholder')}
            aria-label={t('search-placeholder')}
            autocomplete="off"
            minlength="1"
            maxlength="200"
            disabled={isProcessing}
        />
        <small>{isProcessing ? '...' : 'Enter'}</small>
    </form>
</div>

<style lang="scss">
    .search.processing input {
        opacity: 0.55;
        cursor: wait;
    }
</style>
