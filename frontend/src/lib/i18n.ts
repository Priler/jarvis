import { writable, derived } from "svelte/store"
import { getTranslations, getCurrentLanguage, setLanguageInvoke, getSupportedLangs } from "./api"
import { addToast } from "./toast"

// stores
export const translations = writable<Record<string, string>>({})
export const currentLanguage = writable<string>("ru")

// simple helper function (not a store)
export function translate(translations: Record<string, string>, key: string, fallback?: string): string {
    return translations[key] || fallback || key
}

// derived store — use $tStore('key') or $: t = $tStore in components
export const tStore = derived(translations, $trans =>
    (key: string, fallback?: string) => translate($trans, key, fallback)
)

// load translations from backend
export async function loadTranslations() {
    try {
        const [trans, lang] = await Promise.all([getTranslations(), getCurrentLanguage()])
        translations.set(trans)
        currentLanguage.set(lang)
        document.documentElement.lang = lang
    } catch (err: unknown) {
        console.error("Failed to load translations:", err)
        addToast("Failed to load translations — UI text may be unavailable", "error")
    }
}

// change language
export async function setLanguage(lang: string) {
    try {
        const newTranslations = await setLanguageInvoke(lang)
        translations.set(newTranslations)
        currentLanguage.set(lang)
        document.documentElement.lang = lang
    } catch (err: unknown) {
        console.error("Failed to set language:", err)
        addToast("Failed to change language", "error")
    }
}

export async function getSupportedLanguages(): Promise<string[]> {
    try {
        return await getSupportedLangs()
    } catch (err: unknown) {
        console.error("Failed to get supported languages:", err)
        return ["ru", "en", "ua"]
    }
}