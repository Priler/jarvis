import { describe, it, expect, vi, beforeEach } from "vitest"

const {
    mockLoadTranslations,
    mockEnableIpc,
    mockLoadVoiceSetting,
    mockLoadAppInfo,
    mockLoadSettingsSnapshot,
    mockStartStatsPolling,
    mockStartEventTracking,
} = vi.hoisted(() => ({
    mockLoadTranslations:     vi.fn().mockResolvedValue(undefined),
    mockEnableIpc:            vi.fn(),
    mockLoadVoiceSetting:     vi.fn(),
    mockLoadAppInfo:          vi.fn(),
    mockLoadSettingsSnapshot: vi.fn(),
    mockStartStatsPolling:    vi.fn(),
    mockStartEventTracking:   vi.fn(),
}))

vi.mock("@/lib/i18n",                  () => ({ loadTranslations:    mockLoadTranslations }))
vi.mock("@/lib/ipc",                   () => ({ enableIpc:           mockEnableIpc }))
vi.mock("@/lib/stores/voice",          () => ({ loadVoiceSetting:    mockLoadVoiceSetting }))
vi.mock("@/lib/stores/app-info",       () => ({ loadAppInfo:         mockLoadAppInfo }))
vi.mock("@/lib/stores/settings-cache", () => ({ loadSettingsSnapshot: mockLoadSettingsSnapshot }))
vi.mock("@/lib/stores/runtime",        () => ({ startStatsPolling:   mockStartStatsPolling }))
vi.mock("@/lib/stores/event-tracker",  () => ({ startEventTracking:  mockStartEventTracking }))

import { criticalInit, deferredInit } from "@/lib/bootstrap"

beforeEach(() => vi.clearAllMocks())

describe("criticalInit", () => {
    it("awaits loadTranslations before calling enableIpc", async () => {
        const order: string[] = []
        mockLoadTranslations.mockImplementationOnce(async () => { order.push("translations") })
        mockEnableIpc.mockImplementationOnce(() => { order.push("ipc") })
        await criticalInit()
        expect(order).toEqual(["translations", "ipc"])
    })

    it("calls enableIpc", async () => {
        await criticalInit()
        expect(mockEnableIpc).toHaveBeenCalledTimes(1)
    })

    it("calls loadTranslations", async () => {
        await criticalInit()
        expect(mockLoadTranslations).toHaveBeenCalledTimes(1)
    })
})

describe("deferredInit", () => {
    it("calls loadVoiceSetting", () => {
        deferredInit()
        expect(mockLoadVoiceSetting).toHaveBeenCalled()
    })

    it("calls loadAppInfo", () => {
        deferredInit()
        expect(mockLoadAppInfo).toHaveBeenCalled()
    })

    it("calls loadSettingsSnapshot", () => {
        deferredInit()
        expect(mockLoadSettingsSnapshot).toHaveBeenCalled()
    })

    it("calls startStatsPolling with 5000ms interval", () => {
        deferredInit()
        expect(mockStartStatsPolling).toHaveBeenCalledWith(5000)
    })

    it("calls startEventTracking", () => {
        deferredInit()
        expect(mockStartEventTracking).toHaveBeenCalled()
    })

    it("calls all five initializers in a single deferredInit call", () => {
        deferredInit()
        expect(mockLoadVoiceSetting).toHaveBeenCalledTimes(1)
        expect(mockLoadAppInfo).toHaveBeenCalledTimes(1)
        expect(mockLoadSettingsSnapshot).toHaveBeenCalledTimes(1)
        expect(mockStartStatsPolling).toHaveBeenCalledTimes(1)
        expect(mockStartEventTracking).toHaveBeenCalledTimes(1)
    })
})

describe("criticalInit — failure path", () => {
    it("propagates error from loadTranslations and does not call enableIpc", async () => {
        mockLoadTranslations.mockRejectedValueOnce(new Error("i18n failed"))
        await expect(criticalInit()).rejects.toThrow("i18n failed")
        expect(mockEnableIpc).not.toHaveBeenCalled()
    })
})
