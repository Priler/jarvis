<script>
      // IMPORTS
    import { invoke } from "@tauri-apps/api/tauri"
    import { onMount } from 'svelte'
    import { capitalizeFirstLetter } from "@/functions";

    // VARIABLES
    let selected_microphone = 0;
    let microphone_label = "";

    let nn_details = {
        "ww_engine": "",
        "stt_engine": "Vosk"
    }

    // let resources_cpu_temp = 0;
    // let resources_cpu_usage = 0;
    let resources_ram_usage = "-";

    // CODE
    setInterval(() => {
        (async () => {
            resources_ram_usage = Number(await invoke("get_current_ram_usage")).toFixed(2);
            // resources_cpu_temp = await invoke("get_cpu_temp");
            // resources_cpu_usage = +Number(await invoke("get_cpu_usage")).toFixed(2);
        })().catch(err => {
            console.error(err);
        });
    }, 1000);

    onMount(async () => {
        (async () => {
            selected_microphone = +Number(await invoke("db_read", {key: "selected_microphone"}));
            microphone_label = await invoke("pv_get_audio_device_name", {idx: selected_microphone});

            nn_details["ww_engine"] = capitalizeFirstLetter(await invoke("db_read", {key: "selected_wake_word_engine"}));

            // resources_cpu_temp = await invoke("get_cpu_temp");
            // resources_cpu_usage = +Number(await invoke("get_cpu_usage")).toFixed(2);
        })().catch(err => {
            console.error(err);
        });
	});
</script>

<div class="statistics">
    <div class="online">
        <div class="pulse"><div class="wave"></div></div>
        <div class="info">
            <span class="num">Микрофон</span>
            <small title="{microphone_label}">{microphone_label}</small>
        </div>
    </div>
    <div class="files">
        <div class="pulse"><div class="wave"></div></div>
        <div class="info">
            <span class="num">Нейросети</span>
            <small>{nn_details["ww_engine"]} + {nn_details["stt_engine"]}</small>
        </div>
    </div>
    <div class="downloads hint--bottom" aria-label="Общее количество скачиваний по всему проекту">
        <div class="pulse"><div class="wave"></div></div>
        <div class="info">
            <span class="num">Ресурсы</span>
            <small><!-- CPU {resources_cpu_usage}%<br /> -->RAM {resources_ram_usage}mb</small>
        </div>
    </div>
</div>

<style lang="scss">
    .statistics {
        position: relative;
        z-index: 3;
        padding: 0 10px;
        height: 100px;
        display: flex;
        justify-content: space-between;

        & > div {
            height: 70px;
        }

        .info {
            z-index: 10;
        }

        & > .online {
            position: relative;
            width: 40%;

            $base-color: rgba(0, 191, 8, 1);
            $mid-color: rgba(0, 191, 8, 0.4);
            $end-color: rgba(0, 191, 8, 0);

            & > .pulse::before {
                background-color: rgba(0, 191, 8, 1);
            }
            & > .pulse::after {
                background-color: rgba(0, 191, 8, 1);
                animation: online-cdot linear 3s;
                animation-iteration-count: infinite;
                animation-fill-mode: forwards;
            }
            & > .pulse .wave {
                background-color: rgba(0, 191, 8, 0.4);
                animation: online-radarWave cubic-bezier(0, 0.54, 0.53, 1) 3s 0s;
                animation-iteration-count: infinite;
            }
            & > .pulse .wave::after {
                background-color: rgba(0, 191, 8, 0.4);
                animation: online-radarWave cubic-bezier(0, 0.54, 0.53, 1) 3s
                    0.1s;
                animation-iteration-count: infinite;
            }

            & > .info {
                position: absolute;
                top: 26px;
                left: 26px;

                & > span.num {
                    font-size: 18px;
                    font-weight: bold;
                    color: #00bf08;
                }
                & > small {
                    display: block;
                    color: #535a60;
                    font-size: 12px;
                    position: relative;
                    top: 0;
                    width: 130px;
                    max-height: 40px;
                    overflow: hidden;
                    line-height: 1.5em;
                }
            }

            @keyframes online-cdot {
                0% {
                    opacity: 0.3;
                    background: $base-color;
                }
                50% {
                    opacity: 0.5;
                }
                100% {
                    opacity: 1;
                    background: $end-color;
                }
            }
            @keyframes online-radarWave {
                0% {
                    opacity: 0.1;
                    transform: scale(0);
                }
                5% {
                    background: $mid-color;
                    opacity: 1;
                }
                100% {
                    transform: scale(1.2);
                    background: $end-color;
                }
            }
        }

        & > .files {
            position: relative;
            width: 35%;

            $base-color: rgba(255, 129, 48, 1);
            $mid-color: rgba(255, 129, 48, 0.4);
            $end-color: rgba(255, 129, 48, 0);

            & > .pulse::before {
                background-color: $base-color;
            }
            & > .pulse::after {
                background-color: $base-color;
                animation: files-cdot linear 5s;
                animation-iteration-count: infinite;
                animation-fill-mode: forwards;
            }
            & > .pulse .wave {
                background-color: $mid-color;
                animation: files-radarWave cubic-bezier(0, 0.54, 0.53, 1) 5s 0s;
                animation-iteration-count: infinite;
            }
            & > .pulse .wave::after {
                background-color: $mid-color;
                animation: files-radarWave cubic-bezier(0, 0.54, 0.53, 1) 5s
                    0.1s;
                animation-iteration-count: infinite;
            }

            & > .info {
                position: absolute;
                top: 26px;
                left: 26px;

                & > span.num {
                    font-size: 18px;
                    font-weight: bold;
                    color: #ff8130;
                }
                & > small {
                    display: block;
                    color: #535a60;
                    font-size: 12px;
                    position: relative;
                    top: 0;
                }
            }

            @keyframes files-cdot {
                0% {
                    opacity: 0.3;
                    background: $base-color;
                }
                50% {
                    opacity: 0.5;
                }
                100% {
                    opacity: 1;
                    background: $end-color;
                }
            }
            @keyframes files-radarWave {
                0% {
                    opacity: 0.1;
                    transform: scale(0);
                }
                5% {
                    background: $mid-color;
                    transform: scale(0.2);
                    opacity: 1;
                }
                100% {
                    transform: scale(0.8);
                    background: $end-color;
                }
            }
        }

        & > .downloads {
            position: relative;

            $base-color: rgba(11,66,166, 1);
            $mid-color: rgba(32, 150, 243, 0.4);
            $end-color: rgba(32, 150, 243, 0);

            & > .pulse::before {
                background: rgba(32, 150, 243, 1);
            }
            & > .pulse::after {
                background: rgba(32, 150, 243, 1);
                animation: downloads-cdot linear 7s;
                animation-iteration-count: infinite;
                animation-fill-mode: forwards;
                animation-delay: 1s;
            }
            & > .pulse .wave {
                background-color: $mid-color;
                animation: downloads-radarWave cubic-bezier(0, 0.54, 0.53, 1) 7s
                    0s;
                animation-iteration-count: infinite;
                animation-delay: 1s;
            }
            & > .pulse .wave::after {
                background-color: $mid-color;
                animation: downloads-radarWave cubic-bezier(0, 0.54, 0.53, 1) 7s
                    0.1s;
                animation-iteration-count: infinite;
                animation-delay: 1s;
            }

            & > .info {
                position: absolute;
                top: 26px;
                left: 26px;

                & > span.num {
                    font-size: 18px;
                    font-weight: bold;
                    color: #1b78a6;
                }

                & > small {
                    display: block;
                    color: #535a60;
                    font-size: 12px;
                    position: relative;
                    top: 0;
                }
            }

            @keyframes downloads-cdot {
                0% {
                    opacity: 0.3;
                    background: $base-color;
                }
                50% {
                    opacity: 0.5;
                }
                100% {
                    opacity: 1;
                    background: $end-color;
                }
            }
            @keyframes downloads-radarWave {
                0% {
                    opacity: 0.1;
                    transform: scale(0);
                }
                5% {
                    background: $mid-color;
                    opacity: 1;
                }
                100% {
                    transform: scale(0.7);
                    background: $end-color;
                }
            }
        }

        .pulse {
            position: relative;
            height: 100px;
            width: 100px;
            margin: 0;
            left: -43px;
            top: 0px;
            z-index: 5;
        }
        .pulse::before {
            content: '';
            position: absolute;
            width: 11px;
            height: 11px;
            border-radius: 50%;
            left: 50%;
            top: 50%;
            transform: translate(-50%, -50%);
            opacity: .5;
        }
        .pulse::after {
            content: '';
            position: absolute;
            width: 20px;
            height: 20px;
            border-radius: 50%;
            left: 50%;
            top: 50%;
            transform: translate(-50%, -50%);
        }
        .pulse .wave {
            position: absolute;
            left: 7%;
            top: 7%;
            width: 86%;
            height: 86%;
            border-radius: 50%;
            opacity: 0;
        }
        .pulse .wave::after {
            content: '';
            position: absolute;
            left: 7%;
            top: 7%;
            width: 86%;
            height: 86%;
            border-radius: 50%;
            opacity: 0;
        }
    }
</style>
