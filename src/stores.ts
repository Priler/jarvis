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

// etc
export let tg_official_link = "";
export let feedback_link = "";
export let github_repository_link = "";
export let log_file_path = "";

(async () => {
  tg_official_link = await invoke("get_tg_official_link")
  feedback_link = await invoke("get_feedback_link")
  github_repository_link = await invoke("get_repository_link")
  log_file_path = await invoke("get_log_file_path")
})().catch(err => {
    console.error(err);
});