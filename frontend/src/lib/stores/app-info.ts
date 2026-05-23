import { writable } from "svelte/store"
import type { AppInfo } from "@/types"
import {
    getAuthorName,
    getTgOfficialLink, getFeedbackLink, getRepositoryLink,
    getBoostyLink, getPatreonLink, getLogFilePath
} from "@/lib/api"

export const appInfo = writable<AppInfo>({
    authorName:        "",
    tgOfficialLink:    "",
    feedbackLink:      "",
    repositoryLink:    "",
    boostySupportLink: "",
    patreonSupportLink: "",
    logFilePath:       ""
})

let _loaded = false

export async function loadAppInfo() {
    if (_loaded) return
    _loaded = true
    const results = await Promise.allSettled([
        getAuthorName(),
        getTgOfficialLink(),
        getFeedbackLink(),
        getRepositoryLink(),
        getBoostyLink(),
        getPatreonLink(),
        getLogFilePath()
    ])
    const val = (r: PromiseSettledResult<string>) =>
        r.status === 'fulfilled' ? r.value : ""
    appInfo.set({
        authorName:         val(results[0]),
        tgOfficialLink:     val(results[1]),
        feedbackLink:       val(results[2]),
        repositoryLink:     val(results[3]),
        boostySupportLink:  val(results[4]),
        patreonSupportLink: val(results[5]),
        logFilePath:        val(results[6]),
    })
}
