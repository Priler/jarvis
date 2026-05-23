// @vitest-environment happy-dom
import { render, fireEvent } from "@testing-library/svelte"
import { describe, it, expect, vi } from "vitest"
import Select from "@/components/ui/Select.svelte"

const DATA = [
    { label: "Alpha",   value: "a" },
    { label: "Beta",    value: "b" },
    { label: "Gamma",   value: "c" },
]

describe("Select", () => {
    it("renders the selected label", () => {
        const { getByRole } = render(Select, { props: { data: DATA, value: "b" } })
        expect(getByRole("combobox").textContent).toContain("Beta")
    })

    it("opens the dropdown on click", async () => {
        const { getByRole, getAllByRole } = render(Select, { props: { data: DATA, value: "a" } })
        await fireEvent.click(getByRole("combobox"))
        expect(getAllByRole("option")).toHaveLength(3)
    })

    it("closes the dropdown on Escape", async () => {
        const { getByRole, queryAllByRole } = render(Select, { props: { data: DATA, value: "a" } })
        const trigger = getByRole("combobox")
        await fireEvent.click(trigger)
        expect(queryAllByRole("option")).toHaveLength(3)
        const list = getByRole("listbox")
        await fireEvent.keyDown(list, { key: "Escape" })
        expect(queryAllByRole("option")).toHaveLength(0)
    })

    it("selects item on Enter and dispatches change event", async () => {
        const handler = vi.fn()
        const { getByRole, component } = render(Select, { props: { data: DATA, value: "a" } })
        component.$on("change", handler)
        await fireEvent.click(getByRole("combobox"))
        const list = getByRole("listbox")
        await fireEvent.keyDown(list, { key: "ArrowDown" })
        await fireEvent.keyDown(list, { key: "Enter" })
        expect(handler).toHaveBeenCalledWith(expect.objectContaining({ detail: "b" }))
    })

    it("shows label and description when provided", () => {
        const { getByText } = render(Select, {
            props: { data: DATA, value: "a", label: "My Label", description: "Some desc" },
        })
        expect(getByText("My Label")).toBeTruthy()
        expect(getByText("Some desc")).toBeTruthy()
    })

    it("renders overflow message when data exceeds MAX_VISIBLE", async () => {
        const big = Array.from({ length: 205 }, (_, i) => ({ label: `Item ${i}`, value: String(i) }))
        const { getByRole, getByText } = render(Select, { props: { data: big, value: "0" } })
        await fireEvent.click(getByRole("combobox"))
        expect(getByText(/and 5 more/)).toBeTruthy()
    })
})
