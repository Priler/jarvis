// @vitest-environment happy-dom
import { render, fireEvent } from "@testing-library/svelte"
import { describe, it, expect, vi, beforeEach } from "vitest"

vi.mock("@/lib/i18n", async () => {
    const { writable } = await import("svelte/store")
    return { tStore: writable((key: string) => key) }
})

import ConfirmOverlay from "@/components/ui/ConfirmOverlay.svelte"

const pending = { id: "cmd-1", description: "This may be dangerous", cmd: "shutdown /s" }

describe("ConfirmOverlay", () => {
    it("renders nothing when pending is null", () => {
        const { container } = render(ConfirmOverlay, { props: { pending: null } })
        expect(container.querySelector('[role="alertdialog"]')).toBeNull()
    })

    it("shows alertdialog when pending is provided", () => {
        const { getByRole } = render(ConfirmOverlay, { props: { pending } })
        expect(getByRole("alertdialog")).toBeTruthy()
    })

    it("displays the command text", () => {
        const { container } = render(ConfirmOverlay, { props: { pending } })
        expect(container.textContent).toContain("shutdown /s")
    })

    it("displays the description when provided", () => {
        const { container } = render(ConfirmOverlay, { props: { pending } })
        expect(container.textContent).toContain("This may be dangerous")
    })

    it("renders two action buttons", () => {
        const { getAllByRole } = render(ConfirmOverlay, { props: { pending } })
        expect(getAllByRole("button")).toHaveLength(2)
    })

    it("dispatches approve when approve button is clicked", async () => {
        const { component, getAllByRole } = render(ConfirmOverlay, { props: { pending } })
        const onApprove = vi.fn()
        component.$on("approve", onApprove)
        const buttons = getAllByRole("button")
        await fireEvent.click(buttons[buttons.length - 1])
        expect(onApprove).toHaveBeenCalledOnce()
    })

    it("dispatches deny when deny button is clicked", async () => {
        const { component, getAllByRole } = render(ConfirmOverlay, { props: { pending } })
        const onDeny = vi.fn()
        component.$on("deny", onDeny)
        await fireEvent.click(getAllByRole("button")[0])
        expect(onDeny).toHaveBeenCalledOnce()
    })

    it("dispatches approve on Enter key", async () => {
        const { component } = render(ConfirmOverlay, { props: { pending } })
        const onApprove = vi.fn()
        component.$on("approve", onApprove)
        await fireEvent.keyDown(window, { key: "Enter" })
        expect(onApprove).toHaveBeenCalledOnce()
    })

    it("dispatches deny on Escape key", async () => {
        const { component } = render(ConfirmOverlay, { props: { pending } })
        const onDeny = vi.fn()
        component.$on("deny", onDeny)
        await fireEvent.keyDown(window, { key: "Escape" })
        expect(onDeny).toHaveBeenCalledOnce()
    })

    it("does not dispatch events when pending is null", async () => {
        const { component } = render(ConfirmOverlay, { props: { pending: null } })
        const onApprove = vi.fn()
        const onDeny = vi.fn()
        component.$on("approve", onApprove)
        component.$on("deny", onDeny)
        await fireEvent.keyDown(window, { key: "Enter" })
        await fireEvent.keyDown(window, { key: "Escape" })
        expect(onApprove).not.toHaveBeenCalled()
        expect(onDeny).not.toHaveBeenCalled()
    })
})
