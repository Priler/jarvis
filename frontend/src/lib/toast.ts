import { writable } from "svelte/store"

export type ToastType = "error" | "success" | "info"

export interface Toast {
    id:      number
    type:    ToastType
    message: string
}

let _nextId = 0
const DURATION_MS = 4500
const MAX_TOASTS  = 5

export const toasts = writable<Toast[]>([])

export function addToast(message: string, type: ToastType = "error") {
    const id = _nextId++
    toasts.update(ts => {
        const next = [{ id, type, message }, ...ts]
        return next.length > MAX_TOASTS ? next.slice(0, MAX_TOASTS) : next
    })
    setTimeout(() => removeToast(id), DURATION_MS)
}

export function removeToast(id: number) {
    toasts.update(ts => ts.filter(t => t.id !== id))
}
