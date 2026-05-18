<script lang="ts">
    import { onMount, onDestroy } from "svelte"
    import { listen } from "@tauri-apps/api/event"
    import { playSound } from "@/lib/api"
    import { assistantVoice } from "@/stores"
    import { DEFAULT_VOICE_ID } from "@/lib/engine-options"

    $: voiceVal = $assistantVoice || DEFAULT_VOICE_ID

    const SAFE_NAME = /^[a-zA-Z0-9_-]+$/

    let unlisteners: (() => void)[] = []

    onMount(async () => {
        const unlistenAudio = await listen<{ data: string }>("audio-play", async (event) => {
            const voice = voiceVal || "jarvis-remake"
            const rawName = event.payload.data

            if (!SAFE_NAME.test(rawName) || !SAFE_NAME.test(voice)) {
                console.error("[Events] invalid sound path:", voice, rawName)
                return
            }

            try {
                await playSound(`sound/${voice}/${rawName}.wav`)
            } catch (err: unknown) {
                console.error("failed to play sound:", err)
            }
        })

        unlisteners = [unlistenAudio]
    })

    onDestroy(() => {
        unlisteners.forEach(fn => fn())
    })
</script>
