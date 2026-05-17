import { writable, derived } from "svelte/store"
import { getJarvisStats } from "@/lib/api"

interface JarvisStatsState {
    running:   boolean
    ram_mb:    number
    cpu_usage: number
}

export const jarvisStats = writable<JarvisStatsState>({ running: false, ram_mb: 0, cpu_usage: 0 })

export const isJarvisRunning = derived(jarvisStats, s => s.running)
export const jarvisRamUsage  = derived(jarvisStats, s => s.ram_mb)
export const jarvisCpuUsage  = derived(jarvisStats, s => s.cpu_usage)

export async function updateJarvisStats() {
    try {
        const stats = await getJarvisStats()
        jarvisStats.set({ running: stats.running, ram_mb: stats.ram_mb, cpu_usage: stats.cpu_usage })
    } catch (err: unknown) {
        console.error("failed to get jarvis stats:", err)
    }
}

let statsInterval: ReturnType<typeof setInterval> | null = null
let visibilityListener: (() => void) | null = null

export function startStatsPolling(intervalMs = 5000) {
    if (statsInterval) return

    updateJarvisStats()
    statsInterval = setInterval(() => {
        if (!document.hidden) updateJarvisStats()
    }, intervalMs)

    if (!visibilityListener) {
        visibilityListener = () => {
            if (!document.hidden) updateJarvisStats()
        }
        document.addEventListener("visibilitychange", visibilityListener)
    }
}

export function stopStatsPolling() {
    if (statsInterval) {
        clearInterval(statsInterval)
        statsInterval = null
    }
    if (visibilityListener) {
        document.removeEventListener("visibilitychange", visibilityListener)
        visibilityListener = null
    }
}
