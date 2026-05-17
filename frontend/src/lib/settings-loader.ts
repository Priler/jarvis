import { getAppVersion, listVoices, listVoskModels, listGlinerModels } from "./api"
import { loadSettingsValues } from "./settings"
import { LANGUAGE_NAMES } from "./engine-options"
import type { VoiceMeta, SelectOption } from "@/types"
import type { SettingsValues } from "./settings"

export interface SettingsPageData {
    appVersion:           string
    availableVoices:      VoiceMeta[]
    availableVoskModels:  SelectOption[]
    availableGlinerModels: SelectOption[]
    settings:             SettingsValues | null
    errors: {
        meta:     boolean
        voices:   boolean
        vosk:     boolean
        gliner:   boolean
        settings: boolean
    }
}

export async function loadSettingsPageData(): Promise<SettingsPageData> {
    const [appVersion, voices, vosk, gliner, settings] = await Promise.allSettled([
        getAppVersion(),
        listVoices(),
        listVoskModels(),
        listGlinerModels(),
        loadSettingsValues(),
    ])

    return {
        appVersion: appVersion.status === 'fulfilled' ? appVersion.value : "",

        availableVoices: voices.status === 'fulfilled'
            ? voices.value.map(v => v.voice)
            : [],

        availableVoskModels: vosk.status === 'fulfilled'
            ? vosk.value.map(m => ({
                label: `${m.name} (${LANGUAGE_NAMES[m.language] ?? m.language}, ${m.size})`,
                value: m.name,
            }))
            : [],

        availableGlinerModels: gliner.status === 'fulfilled'
            ? gliner.value.map(m => ({ label: m.display_name, value: m.value }))
            : [],

        settings: settings.status === 'fulfilled' ? settings.value : null,

        errors: {
            meta:     appVersion.status === 'rejected',
            voices:   voices.status     === 'rejected',
            vosk:     vosk.status       === 'rejected',
            gliner:   gliner.status     === 'rejected',
            settings: settings.status   === 'rejected',
        }
    }
}
