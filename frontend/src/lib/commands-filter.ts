import type { JCommand } from "@/types"

/**
 * Return the phrase list for `cmd` in the given language,
 * falling back to English, then to whatever language is first.
 */
export function getPhrases(cmd: JCommand, lang: string): string[] {
    return cmd.phrases[lang] ?? cmd.phrases["en"] ?? Object.values(cmd.phrases)[0] ?? []
}

/**
 * Filter commands by a free-text query against command IDs and phrases.
 * Returns the original array when query is empty (no allocation).
 */
export function filterCommands(
    commands: JCommand[],
    query:    string,
    lang:     string,
): JCommand[] {
    const q = query.trim().toLowerCase()
    if (!q) return commands
    return commands.filter(cmd =>
        cmd.id.toLowerCase().includes(q) ||
        getPhrases(cmd, lang).some(p => p.toLowerCase().includes(q))
    )
}
