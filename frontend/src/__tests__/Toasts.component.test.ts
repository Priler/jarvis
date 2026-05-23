// @vitest-environment happy-dom
import { render, fireEvent, waitFor } from "@testing-library/svelte"
import { describe, it, expect, vi, beforeEach } from "vitest"
import { get } from "svelte/store"
import { toasts, addToast, removeToast } from "@/lib/toast"
import Toasts from "@/components/ui/Toasts.svelte"

beforeEach(() => {
    toasts.set([])
})

describe("Toasts", () => {
    it("renders nothing when there are no toasts", () => {
        const { queryAllByRole } = render(Toasts)
        expect(queryAllByRole("alert")).toHaveLength(0)
    })

    it("renders a toast when addToast is called", async () => {
        addToast("Hello world", "info")
        const { findAllByRole } = render(Toasts)
        const alerts = await findAllByRole("alert")
        expect(alerts).toHaveLength(1)
        expect(alerts[0].textContent).toContain("Hello world")
    })

    it("renders multiple toasts", async () => {
        addToast("First",  "success")
        addToast("Second", "error")
        const { findAllByRole } = render(Toasts)
        expect(await findAllByRole("alert")).toHaveLength(2)
    })

    it("removes a toast when dismiss button is clicked", async () => {
        addToast("Dismiss me", "error")
        const { findByRole, queryAllByRole } = render(Toasts)
        const btn = await findByRole("button")
        await fireEvent.click(btn)
        // out:fly transition keeps element in DOM briefly; wait for it to clear
        await waitFor(() => expect(queryAllByRole("alert")).toHaveLength(0), { timeout: 2000 })
    })

    it("removeToast removes correct toast from store", () => {
        addToast("A", "info")
        addToast("B", "info")
        const [first] = get(toasts)
        removeToast(first.id)
        const remaining = get(toasts)
        expect(remaining).toHaveLength(1)
        expect(remaining[0].message).toBe("A")
    })
})
