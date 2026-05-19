// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach } from "vitest"

// Hoist mutable refs so mock factories and tests share the same objects.
const mocks = vi.hoisted(() => {
    const gotoFn    = vi.fn()
    const isActiveFn = vi.fn().mockReturnValue(false)
    return { gotoFn, isActiveFn }
})

// routify stores expose the function as the store value; $goto / $isActive unwrap it.
vi.mock("@roxi/routify", () => ({
    goto:     { subscribe: (fn: (v: unknown) => void) => { fn(mocks.gotoFn);     return () => {} } },
    isActive: { subscribe: (fn: (v: unknown) => void) => { fn(mocks.isActiveFn); return () => {} } },
}))

vi.mock("@/stores", async () => {
    const { writable } = await import("svelte/store")
    return { tStore: writable((key: string) => key) }
})

// WindowFrame uses getCurrentWindow in onMount; stub it so tests don't error.
vi.mock("@tauri-apps/api/window", () => ({
    getCurrentWindow: vi.fn(() => ({
        isMaximized:      vi.fn().mockResolvedValue(false),
        onResized:        vi.fn().mockResolvedValue(() => {}),
        minimize:         vi.fn(),
        toggleMaximize:   vi.fn(),
        close:            vi.fn(),
    })),
}))

import { render, fireEvent } from "@testing-library/svelte"
import Header from "@/components/Header.svelte"

beforeEach(() => {
    vi.clearAllMocks()
    mocks.isActiveFn.mockReturnValue(false)
})

describe("Header", () => {
    it("renders the JARVIS brand name", () => {
        const { container } = render(Header)
        expect(container.textContent).toContain("JARVIS")
    })

    it("renders a navigation landmark", () => {
        const { getByRole } = render(Header)
        expect(getByRole("navigation")).toBeTruthy()
    })

    it("renders links for all four routes", () => {
        const { getAllByRole } = render(Header)
        const hrefs = getAllByRole("link").map(l => l.getAttribute("href"))
        expect(hrefs).toContain("/")
        expect(hrefs).toContain("/commands")
        expect(hrefs).toContain("/settings")
        expect(hrefs).toContain("/system")
    })

    it("calls goto when a nav link is clicked", async () => {
        const { getAllByRole } = render(Header)
        const navLinks = getAllByRole("link")
        await fireEvent.click(navLinks[0])
        expect(mocks.gotoFn).toHaveBeenCalled()
    })

    it("applies aria-current=page to the active link", () => {
        mocks.isActiveFn.mockReturnValue(true)
        const { getAllByRole } = render(Header)
        const activeLinks = getAllByRole("link").filter(
            l => l.getAttribute("aria-current") === "page"
        )
        expect(activeLinks.length).toBeGreaterThan(0)
    })

    it("no link has aria-current when none is active", () => {
        mocks.isActiveFn.mockReturnValue(false)
        const { getAllByRole } = render(Header)
        const activeLinks = getAllByRole("link").filter(
            l => l.getAttribute("aria-current") === "page"
        )
        expect(activeLinks).toHaveLength(0)
    })

    it("renders window control buttons inside the shell bar", () => {
        const { getAllByRole } = render(Header)
        const buttons = getAllByRole("button")
        expect(buttons.length).toBeGreaterThanOrEqual(2) // min/max/close
    })
})
