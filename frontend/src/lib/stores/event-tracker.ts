import { jarvisState, lastRecognizedText, lastExecutedCommand, lastError } from "@/lib/ipc"
import { addRuntimeEvent } from "./events"

function skipFirst<T>(cb: (v: T) => void): (v: T) => void {
    let first = true
    return (v: T) => { if (first) { first = false; return } cb(v) }
}

let unsubs: (() => void)[] = []

export function startEventTracking(): void {
    if (unsubs.length > 0) return

    unsubs.push(jarvisState.subscribe(skipFirst(state => {
        if (state === 'listening')  addRuntimeEvent('WAKE WORD DETECTED')
        if (state === 'processing') addRuntimeEvent('PROCESSING SPEECH')
        if (state === 'idle')       addRuntimeEvent('SYSTEM IDLE')
    })))

    unsubs.push(lastRecognizedText.subscribe(skipFirst(text => {
        if (text) addRuntimeEvent('SPEECH RECOGNIZED', text)
    })))

    unsubs.push(lastExecutedCommand.subscribe(skipFirst(cmd => {
        if (cmd) addRuntimeEvent('COMMAND EXECUTED', cmd.id)
    })))

    unsubs.push(lastError.subscribe(skipFirst(err => {
        if (err) addRuntimeEvent('ERROR', err)
    })))
}

export function stopEventTracking(): void {
    unsubs.forEach(u => u())
    unsubs = []
}
