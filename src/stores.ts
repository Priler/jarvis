import { invoke } from "@tauri-apps/api/tauri"
import { writable } from 'svelte/store'

// listen state
export const is_listening = writable(true);
let is_listening__val: boolean;
is_listening.subscribe(value => {
  is_listening__val = value;
});
export function isListening() {return is_listening__val}

// assistant voice
export const assistant_voice = writable("");

(async () => {
  assistant_voice.set(await invoke("db_read", {key: "assistant_voice"}));
})().catch(err => {
    console.error(err);
});