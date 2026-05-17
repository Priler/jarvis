// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach } from "vitest"

// All vi.fn() must be created inside the factory or via vi.hoisted — never reference outer const.
vi.mock("@/stores", async () => {
    const { writable } = await import("svelte/store")
    return {
        isJarvisRunning:  writable(true),
        ipcConnected:     writable(true),
        tStore:           writable((_key: string, fallback = _key) => fallback),
        sendTextCommand:  vi.fn(() => true),
    }
})

vi.mock("@/lib/toast", () => ({
    addToast: vi.fn(),
}))

import { render, fireEvent } from "@testing-library/svelte"
import SearchBar from "@/components/elements/SearchBar.svelte"
import * as stores from "@/stores"
import * as toastLib from "@/lib/toast"

const sendCmd  = () => vi.mocked(stores.sendTextCommand)
const addToast = () => vi.mocked(toastLib.addToast)

beforeEach(() => {
    vi.clearAllMocks()
    sendCmd().mockReturnValue(true)
    ;(stores.isJarvisRunning as any).set(true)
    ;(stores.ipcConnected    as any).set(true)
})

describe("SearchBar", () => {
    it("renders an input", () => {
        const { getByRole } = render(SearchBar)
        expect(getByRole("textbox")).toBeTruthy()
    })

    it("clears input on Escape", async () => {
        const { getByRole } = render(SearchBar)
        const input = getByRole("textbox") as HTMLInputElement
        await fireEvent.input(input, { target: { value: "hello" } })
        await fireEvent.keyDown(input, { key: "Escape" })
        expect(input.value).toBe("")
    })

    it("calls sendTextCommand on form submit when running", async () => {
        const { getByRole } = render(SearchBar)
        const input = getByRole("textbox") as HTMLInputElement
        await fireEvent.input(input, { target: { value: "test command" } })
        await fireEvent.submit(input.closest("form")!)
        expect(sendCmd()).toHaveBeenCalledWith("test command")
    })

    it("shows an error toast when jarvis is not running", async () => {
        ;(stores.isJarvisRunning as any).set(false)
        const { getByRole } = render(SearchBar)
        const input = getByRole("textbox") as HTMLInputElement
        await fireEvent.input(input, { target: { value: "cmd" } })
        await fireEvent.submit(input.closest("form")!)
        expect(addToast()).toHaveBeenCalled()
        expect(sendCmd()).not.toHaveBeenCalled()
    })

    it("does not submit empty input", async () => {
        const { getByRole } = render(SearchBar)
        const input = getByRole("textbox") as HTMLInputElement
        await fireEvent.submit(input.closest("form")!)
        expect(sendCmd()).not.toHaveBeenCalled()
    })
})
