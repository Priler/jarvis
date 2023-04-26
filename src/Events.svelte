<script>
    import { onMount, onDestroy } from 'svelte'
    import { emit, listen } from '@tauri-apps/api/event'

    import { resolveResource } from '@tauri-apps/api/path'

    import {Howl, Howler} from 'howler';

    let assistant_voice_val = "jarvis-og";
    import { assistant_voice } from "@/stores"
    import { invoke } from '@tauri-apps/api/tauri';
    assistant_voice.subscribe(value => {
        assistant_voice_val = value;
    });

    onMount(async () => {
        await listen('audio-play', async (event) => {
            // event.event is the event name (useful if you want to use a single callback fn for multiple event types)
            // event.payload is the payload object
            // let path = await resolveResource('sound/' + (assistant_voice_val == "" ? "jarvis-remake":assistant_voice_val) + '/' + event.payload['data'] + '.wav');
            // console.log(path);
            // let sound = new Howl({
            //     src: [path],
            //     html5: true
            // });

            // sound.play();

            let filename = 'sound/' + (assistant_voice_val == "" ? "jarvis-remake":assistant_voice_val) + '/' + event.payload['data'] + '.wav';
            await invoke("play_sound", {
                filename: filename,
                sleep: true
            });
        });

        await listen('assistant-greet', (event) => {
            document.getElementById("arc-reactor").classList.add("active");
        });

        await listen('assistant-waiting', (event) => {
            document.getElementById("arc-reactor").classList.remove("active");
        });
	});
</script>