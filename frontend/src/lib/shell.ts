import { open } from "@tauri-apps/plugin-shell"
import { showInFolder } from "./api"
import { addToast } from "./toast"

export function showInExplorer(path: string): void {
    showInFolder(path)
        .catch(err => {
            console.error("failed to open explorer:", err)
            addToast("Failed to open file location", "error")
        })
}

export function openUrl(url: string): void {
    if (!url) return
    open(url).catch(err => {
        console.error("failed to open URL:", err)
    })
}
