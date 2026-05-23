// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach } from "vitest"
import { render } from "@testing-library/svelte"

const { jarvisState } = vi.hoisted(() => {
    function makeWritable<T>(initial: T) {
        let val = initial
        const subs: ((v: T) => void)[] = []
        return {
            subscribe(fn: (v: T) => void) {
                subs.push(fn)
                fn(val)
                return () => { const i = subs.indexOf(fn); if (i >= 0) subs.splice(i, 1) }
            },
            set(v: T) { val = v; subs.forEach(fn => fn(v)) },
            update(fn: (v: T) => T) { val = fn(val); subs.forEach(fn2 => fn2(val)) },
        }
    }
    return { jarvisState: makeWritable<string>("disconnected") }
})

vi.mock("@/stores", () => ({ jarvisState }))

import ArcReactor from "@/components/elements/ArcReactor.svelte"

beforeEach(() => {
    jarvisState.set("disconnected")
})

function reactor(state: string) {
    jarvisState.set(state)
    const { container } = render(ArcReactor)
    return container.querySelector("#arc-reactor")!
}

describe("ArcReactor", () => {
    it("renders the arc-reactor element", () => {
        const { container } = render(ArcReactor)
        expect(container.querySelector("#arc-reactor")).not.toBeNull()
    })

    it("has aria-hidden to keep it out of the accessibility tree", () => {
        const { container } = render(ArcReactor)
        expect(container.querySelector("#arc-reactor")?.getAttribute("aria-hidden")).toBe("true")
    })

    it("applies disconnected state class when state is disconnected", () => {
        expect(reactor("disconnected").className).toContain("disconnected")
    })

    it("applies idle state class when state is idle", () => {
        expect(reactor("idle").className).toContain("idle")
    })

    it("applies active and s-listening classes when state is listening", () => {
        const el = reactor("listening")
        expect(el.className).toContain("active")
        expect(el.className).toContain("s-listening")
    })

    it("applies active and s-processing classes when state is processing", () => {
        const el = reactor("processing")
        expect(el.className).toContain("active")
        expect(el.className).toContain("s-processing")
    })

    it("applies arc-white color class when disconnected", () => {
        expect(reactor("disconnected").className).toContain("arc-white")
    })

    it("applies arc-cyan color class when idle", () => {
        expect(reactor("idle").className).toContain("arc-cyan")
    })

    it("applies arc-cyan color class when listening", () => {
        expect(reactor("listening").className).toContain("arc-cyan")
    })

    it("applies arc-cyan color class when processing", () => {
        expect(reactor("processing").className).toContain("arc-cyan")
    })
})
