<script lang="ts">
    import Toolbar from '$lib/components/Toolbar.svelte';
    import SidePanel from '$lib/components/SidePanel.svelte';
    import { bridge } from '$lib/bridge';
    import { onMount } from 'svelte';

    let renderStats = $state({
        fps: 0,
        frameTime: 0,
    });

    onMount(() => {
        // Subscribe to messages from Bevy
        const unsubscribe = bridge.subscribe((msg) => {
            if (msg.type === 'RenderStats') {
                renderStats = {
                    fps: msg.data.fps,
                    frameTime: msg.data.frame_time_ms,
                };
            }
        });

        return unsubscribe;
    });
</script>

<div class="app">
    <Toolbar {renderStats} />
    <SidePanel />
</div>

<style>
    .app {
        width: 100vw;
        height: 100vh;
        pointer-events: none;
    }

    .app :global(*) {
        pointer-events: auto;
    }
</style>
