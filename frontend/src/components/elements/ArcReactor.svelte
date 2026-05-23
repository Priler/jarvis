<script lang="ts">
    import { jarvisState } from "@/stores"

    const TICK_COUNT = 60
    const COIL_COUNT = 8

    $: stateClass = {
        'disconnected': 'disconnected',
        'idle':         'idle',
        'listening':    'active s-listening',
        'processing':   'active s-processing'
    }[$jarvisState] || 'disconnected'

    $: colorClass = {
        'disconnected': 'arc-white',
        'idle':         'arc-cyan',
        'listening':    'arc-cyan',
        'processing':   'arc-cyan'
    }[$jarvisState] || 'arc-white'
</script>

<div id="arc-reactor" class="reactor-container {stateClass} {colorClass}" aria-hidden="true">
    <div class="reactor-container-inner circle abs-center">
        <ul class="marks">
            {#each Array(TICK_COUNT) as _, i}
                <li></li>
            {/each}
        </ul>
        <div class="e7">
            <div class="semi_arc_3 e5_1">
                <div class="semi_arc_3 e5_1_ghost"></div>
                <div class="semi_arc_3 e5_2">
                    <div class="semi_arc_3 e5_2_ghost"></div>
                    <div class="semi_arc_3 e5_3">
                        <div class="semi_arc_3 e5_3_ghost"></div>
                        <div class="semi_arc_3 e5_4">
                            <div class="semi_arc_3 e5_4_ghost"></div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>
    <div class="tunnel circle abs-center"></div>
    <div class="core-wrapper circle abs-center"></div>
    <div class="core-outer circle abs-center"></div>
    <div class="core-inner circle abs-center"></div>
    <div class="coil-container">
        {#each Array(COIL_COUNT) as _, i}
            <div class="coil coil-{i + 1}"></div>
        {/each}
    </div>
</div>
