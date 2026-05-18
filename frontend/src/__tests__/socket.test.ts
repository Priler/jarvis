import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { get } from "svelte/store"

class MockWebSocket {
    static readonly CONNECTING = 0
    static readonly OPEN       = 1
    static readonly CLOSING    = 2
    static readonly CLOSED     = 3
    static last: MockWebSocket | null = null

    readyState = MockWebSocket.CONNECTING
    sent: string[] = []
    readonly url: string

    onopen:    ((e: Event) => void)        | null = null
    onclose:   ((e: CloseEvent) => void)   | null = null
    onerror:   ((e: Event) => void)        | null = null
    onmessage: ((e: MessageEvent) => void) | null = null

    constructor(url: string) {
        this.url = url
        MockWebSocket.last = this
    }

    send(data: string) { this.sent.push(data) }

    close() {
        if (this.readyState !== MockWebSocket.CLOSED) {
            this.readyState = MockWebSocket.CLOSED
            this.onclose?.({} as CloseEvent)
        }
    }

    open() {
        this.readyState = MockWebSocket.OPEN
        this.onopen?.(new Event("open"))
    }

    msg(data: object) {
        this.onmessage?.({ data: JSON.stringify(data) } as MessageEvent)
    }
}

vi.mock("@tauri-apps/api/window", () => ({
    getCurrentWindow: vi.fn(() => ({
        show:       vi.fn().mockResolvedValue(undefined),
        unminimize: vi.fn().mockResolvedValue(undefined),
        setFocus:   vi.fn().mockResolvedValue(undefined),
    })),
}))

vi.stubGlobal("WebSocket", MockWebSocket)

import { jarvisState, ipcConnected, lastRecognizedText, lastExecutedCommand, lastError, pendingConfirmation, sandboxWarnings } from "@/lib/ipc/stores"
import { connectIpc, disconnectIpc, enableIpc, sendTextCommand, sendConfirmResult, setIpcToken, stopJarvisApp, reloadCommands } from "@/lib/ipc/socket"

function connect(port?: number) {
    connectIpc(port)
    MockWebSocket.last!.open()
}

beforeEach(() => {
    vi.useFakeTimers()
    MockWebSocket.last = null
    disconnectIpc()
    lastRecognizedText.set("")
    lastExecutedCommand.set(null)
    lastError.set("")
    pendingConfirmation.set(null)
    sandboxWarnings.set([])
    setIpcToken(null)
})

afterEach(() => {
    disconnectIpc()
    vi.useRealTimers()
})

describe("connectIpc", () => {
    it("creates WebSocket at ws://127.0.0.1:9712 by default", () => {
        connectIpc()
        expect(MockWebSocket.last?.url).toBe("ws://127.0.0.1:9712")
    })

    it("respects custom port", () => {
        connectIpc(8888)
        expect(MockWebSocket.last?.url).toBe("ws://127.0.0.1:8888")
    })

    it("sets ipcConnected to true on open", () => {
        connectIpc()
        expect(get(ipcConnected)).toBe(false)
        MockWebSocket.last!.open()
        expect(get(ipcConnected)).toBe(true)
    })

    it("sets jarvisState to idle on open", () => {
        connect()
        expect(get(jarvisState)).toBe("idle")
    })

    it("clears lastError on open", () => {
        lastError.set("previous error")
        connect()
        expect(get(lastError)).toBe("")
    })

    it("sets ipcConnected to false on close", () => {
        connect()
        MockWebSocket.last!.close()
        expect(get(ipcConnected)).toBe(false)
    })

    it("sets jarvisState to disconnected on close", () => {
        connect()
        MockWebSocket.last!.close()
        expect(get(jarvisState)).toBe("disconnected")
    })

    it("does not create a second socket while CONNECTING", () => {
        connectIpc()
        const first = MockWebSocket.last
        connectIpc()
        expect(MockWebSocket.last).toBe(first)
    })

    it("does not create a second socket while OPEN", () => {
        connect()
        const first = MockWebSocket.last
        connectIpc()
        expect(MockWebSocket.last).toBe(first)
    })
})

describe("IPC event handling", () => {
    beforeEach(() => connect())

    it("sets jarvisState to listening on wake_word_detected", () => {
        MockWebSocket.last!.msg({ event: "wake_word_detected" })
        expect(get(jarvisState)).toBe("listening")
    })

    it("sets jarvisState to listening on listening event", () => {
        MockWebSocket.last!.msg({ event: "listening" })
        expect(get(jarvisState)).toBe("listening")
    })

    it("sets jarvisState to processing and updates text on speech_recognized", () => {
        MockWebSocket.last!.msg({ event: "speech_recognized", text: "hello world" })
        expect(get(jarvisState)).toBe("processing")
        expect(get(lastRecognizedText)).toBe("hello world")
    })

    it("updates lastExecutedCommand on command_executed", () => {
        MockWebSocket.last!.msg({ event: "command_executed", id: "music.play" })
        const cmd = get(lastExecutedCommand)
        expect(cmd?.id).toBe("music.play")
        expect(typeof cmd?.seq).toBe("number")
    })

    it("increments seq for the same command to force re-notification", () => {
        MockWebSocket.last!.msg({ event: "command_executed", id: "music.play" })
        const seq1 = get(lastExecutedCommand)!.seq
        MockWebSocket.last!.msg({ event: "command_executed", id: "music.play" })
        const seq2 = get(lastExecutedCommand)!.seq
        expect(seq2).toBeGreaterThan(seq1)
    })

    it("sets jarvisState to idle on idle event", () => {
        MockWebSocket.last!.msg({ event: "idle" })
        expect(get(jarvisState)).toBe("idle")
    })

    it("sets lastError on error event", () => {
        MockWebSocket.last!.msg({ event: "error", message: "STT failed" })
        expect(get(lastError)).toBe("STT failed")
    })

    it("sets lastError to Unknown error when message is missing", () => {
        MockWebSocket.last!.msg({ event: "error" })
        expect(get(lastError)).toBe("Unknown error")
    })

    it("sets jarvisState to idle on started event", () => {
        MockWebSocket.last!.msg({ event: "started" })
        expect(get(jarvisState)).toBe("idle")
    })

    it("sets jarvisState to disconnected on stopping event", () => {
        MockWebSocket.last!.msg({ event: "stopping" })
        expect(get(jarvisState)).toBe("disconnected")
    })
})

describe("processing timeout", () => {
    it("forces idle after 60 seconds of processing", () => {
        connect()
        MockWebSocket.last!.msg({ event: "speech_recognized", text: "test" })
        expect(get(jarvisState)).toBe("processing")
        vi.advanceTimersByTime(60_000)
        expect(get(jarvisState)).toBe("idle")
    })

    it("idle event clears the timeout — state stays idle and does not flip", () => {
        connect()
        MockWebSocket.last!.msg({ event: "speech_recognized", text: "test" })
        MockWebSocket.last!.msg({ event: "idle" })
        // Heartbeat fires at 30s when state is idle; skip timer advance to avoid interference
        expect(get(jarvisState)).toBe("idle")
    })

    it("command_executed clears the timeout — state stays processing past 60s", () => {
        connect()
        MockWebSocket.last!.msg({ event: "speech_recognized", text: "test" })
        MockWebSocket.last!.msg({ event: "command_executed", id: "lights.off" })
        // Heartbeat skips while state === "processing", so advancing 60s only
        // fires the processing timeout — which was cleared by command_executed
        vi.advanceTimersByTime(60_000)
        expect(get(jarvisState)).toBe("processing")
    })
})

describe("sendTextCommand", () => {
    it("sends immediately when connected and returns true", () => {
        connect()
        const result = sendTextCommand("turn off lights")
        expect(result).toBe(true)
        const last = JSON.parse(MockWebSocket.last!.sent.at(-1)!)
        expect(last).toMatchObject({ action: "text_command", text: "turn off lights" })
    })

    it("returns false and queues command when disconnected", () => {
        const result = sendTextCommand("turn off lights")
        expect(result).toBe(false)
    })

    it("flushes pending commands after reconnect", () => {
        sendTextCommand("first")
        sendTextCommand("second")
        connect()
        vi.advanceTimersByTime(500)
        const textCmds = MockWebSocket.last!.sent
            .map(s => JSON.parse(s))
            .filter(s => s.action === "text_command")
        expect(textCmds.map(s => s.text)).toEqual(["first", "second"])
    })

    it("caps pending queue at 20 — oldest commands are dropped", () => {
        for (let i = 0; i < 22; i++) sendTextCommand(`cmd-${i}`)
        connect()
        vi.advanceTimersByTime(5_000)
        const textCmds = MockWebSocket.last!.sent
            .map(s => JSON.parse(s))
            .filter(s => s.action === "text_command")
        expect(textCmds).toHaveLength(20)
        expect(textCmds[0].text).toBe("cmd-2")
        expect(textCmds[19].text).toBe("cmd-21")
    })
})

describe("disconnectIpc", () => {
    it("sets ipcConnected to false", () => {
        connect()
        disconnectIpc()
        expect(get(ipcConnected)).toBe(false)
    })

    it("sets jarvisState to disconnected", () => {
        connect()
        disconnectIpc()
        expect(get(jarvisState)).toBe("disconnected")
    })

    it("clears pending commands", () => {
        sendTextCommand("queued")
        disconnectIpc()
        connect()
        vi.advanceTimersByTime(500)
        const textCmds = MockWebSocket.last!.sent
            .map(s => JSON.parse(s))
            .filter(s => s.action === "text_command")
        expect(textCmds).toHaveLength(0)
    })

    it("prevents automatic reconnect after manual disconnect", () => {
        enableIpc()
        MockWebSocket.last!.open()
        const first = MockWebSocket.last
        disconnectIpc()
        vi.advanceTimersByTime(5_000)
        expect(MockWebSocket.last).toBe(first)
    })
})

describe("stopJarvisApp", () => {
    it("sends stop action and returns true when connected", () => {
        connect()
        const result = stopJarvisApp()
        expect(result).toBe(true)
        const last = JSON.parse(MockWebSocket.last!.sent.at(-1)!)
        expect(last).toMatchObject({ action: "stop" })
    })

    it("returns false when disconnected", () => {
        expect(stopJarvisApp()).toBe(false)
    })
})

describe("reloadCommands", () => {
    it("sends reload_commands action and returns true when connected", () => {
        connect()
        const result = reloadCommands()
        expect(result).toBe(true)
        const last = JSON.parse(MockWebSocket.last!.sent.at(-1)!)
        expect(last).toMatchObject({ action: "reload_commands" })
    })

    it("returns false when disconnected", () => {
        expect(reloadCommands()).toBe(false)
    })
})

describe("confirmation_required event", () => {
    beforeEach(() => connect())

    it("sets pendingConfirmation store on confirmation_required event", () => {
        MockWebSocket.last!.msg({
            event: "confirmation_required",
            id: "jarvis_reboot",
            description: "Перезагрузка компьютера",
            cmd: "shutdown /r /t 0",
        })
        const p = get(pendingConfirmation)
        expect(p).not.toBeNull()
        expect(p?.id).toBe("jarvis_reboot")
        expect(p?.description).toBe("Перезагрузка компьютера")
        expect(p?.cmd).toBe("shutdown /r /t 0")
    })
})

describe("sandbox_warning event", () => {
    beforeEach(() => connect())

    it("sets sandboxWarnings store on sandbox_warning event", () => {
        MockWebSocket.last!.msg({ event: "sandbox_warning", commands: ["dangerous_cmd"] })
        expect(get(sandboxWarnings)).toEqual(["dangerous_cmd"])
    })

    it("replaces previous sandbox warnings", () => {
        sandboxWarnings.set(["old"])
        MockWebSocket.last!.msg({ event: "sandbox_warning", commands: ["new1", "new2"] })
        expect(get(sandboxWarnings)).toEqual(["new1", "new2"])
    })
})

describe("sendConfirmResult", () => {
    it("sends confirm_result action with approved=true and returns true when connected", () => {
        connect()
        const result = sendConfirmResult("jarvis_reboot", true)
        expect(result).toBe(true)
        const last = JSON.parse(MockWebSocket.last!.sent.at(-1)!)
        expect(last).toMatchObject({ action: "confirm_result", id: "jarvis_reboot", approved: true })
    })

    it("sends confirm_result action with approved=false", () => {
        connect()
        sendConfirmResult("jarvis_reboot", false)
        const last = JSON.parse(MockWebSocket.last!.sent.at(-1)!)
        expect(last).toMatchObject({ action: "confirm_result", id: "jarvis_reboot", approved: false })
    })

    it("returns false when disconnected", () => {
        expect(sendConfirmResult("jarvis_reboot", true)).toBe(false)
    })
})

describe("auth token", () => {
    it("does not send auth message when no token is set", () => {
        connect()
        const sent = MockWebSocket.last!.sent.map(s => JSON.parse(s))
        expect(sent.some(s => s.action === "auth")).toBe(false)
    })

    it("sends auth message as first message when token is set", () => {
        setIpcToken("test-secret-token")
        connect()
        const first = JSON.parse(MockWebSocket.last!.sent[0])
        expect(first).toMatchObject({ action: "auth", token: "test-secret-token" })
    })
})

describe("pong event", () => {
    it("clears the heartbeat timeout so no forced reconnect fires", () => {
        connect()
        // Advance to heartbeat interval — ping sent, heartbeatTimeoutTimer armed
        vi.advanceTimersByTime(30_000)
        const sentBeforePong = MockWebSocket.last!.sent.length
        // pong arrives — clears heartbeatTimeoutTimer
        MockWebSocket.last!.msg({ event: "pong" })
        // Advance past the 5s heartbeat timeout window — connection should stay open
        vi.advanceTimersByTime(5_000)
        expect(MockWebSocket.last!.readyState).toBe(MockWebSocket.OPEN)
        expect(MockWebSocket.last!.sent.length).toBe(sentBeforePong)
    })

    it("forces reconnect when pong is not received within 5s", () => {
        connect()
        vi.advanceTimersByTime(30_000) // heartbeat fires, timeout armed
        vi.advanceTimersByTime(5_000)  // no pong — timeout closes connection
        expect(MockWebSocket.last!.readyState).toBe(MockWebSocket.CLOSED)
    })
})
