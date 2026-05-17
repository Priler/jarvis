import { showInFolder } from "./api"
import { addToast } from "./toast"

export function showInExplorer(path: string): void {
    showInFolder(path)
        .catch(err => {
            console.error("failed to open explorer:", err)
            addToast("Failed to open file location", "error")
        })
}
