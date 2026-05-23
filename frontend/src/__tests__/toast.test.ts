import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { get } from "svelte/store"
import { toasts, addToast, removeToast } from "@/lib/toast"

beforeEach(() => {
    vi.useFakeTimers()
    toasts.set([])
})

afterEach(() => {
    vi.useRealTimers()
})

describe("addToast", () => {
    it("adds a toast with the correct message and type", () => {
        addToast("hello", "info")
        const list = get(toasts)
        expect(list).toHaveLength(1)
        expect(list[0].message).toBe("hello")
        expect(list[0].type).toBe("info")
    })

    it("defaults type to error", () => {
        addToast("oops")
        expect(get(toasts)[0].type).toBe("error")
    })

    it("prepends new toasts (most recent first)", () => {
        addToast("first", "info")
        addToast("second", "info")
        const list = get(toasts)
        expect(list[0].message).toBe("second")
        expect(list[1].message).toBe("first")
    })

    it("caps at MAX_TOASTS (5)", () => {
        for (let i = 0; i < 7; i++) addToast(`msg-${i}`, "info")
        expect(get(toasts)).toHaveLength(5)
    })

    it("keeps the most recent toasts when cap is hit", () => {
        for (let i = 0; i < 7; i++) addToast(`msg-${i}`, "info")
        const messages = get(toasts).map(t => t.message)
        expect(messages).not.toContain("msg-0")
        expect(messages).not.toContain("msg-1")
        expect(messages[0]).toBe("msg-6")
    })

    it("auto-removes after 4500ms", () => {
        addToast("auto", "info")
        expect(get(toasts)).toHaveLength(1)
        vi.advanceTimersByTime(4500)
        expect(get(toasts)).toHaveLength(0)
    })

    it("does not remove before 4500ms", () => {
        addToast("auto", "info")
        vi.advanceTimersByTime(4499)
        expect(get(toasts)).toHaveLength(1)
    })

    it("assigns unique ids across multiple calls", () => {
        addToast("a", "info")
        addToast("b", "info")
        addToast("c", "error")
        const ids = get(toasts).map(t => t.id)
        expect(new Set(ids).size).toBe(3)
    })
})

describe("removeToast", () => {
    it("removes a toast by id", () => {
        addToast("remove me", "info")
        const id = get(toasts)[0].id
        removeToast(id)
        expect(get(toasts)).toHaveLength(0)
    })

    it("removes only the targeted toast when multiple exist", () => {
        addToast("a", "info")
        addToast("b", "info")
        const idA = get(toasts)[1].id
        removeToast(idA)
        const remaining = get(toasts)
        expect(remaining).toHaveLength(1)
        expect(remaining[0].message).toBe("b")
    })

    it("is a no-op for an unknown id", () => {
        addToast("keep me", "info")
        removeToast(99999)
        expect(get(toasts)).toHaveLength(1)
    })
})
