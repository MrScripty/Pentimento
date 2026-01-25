<script lang="ts">
    import Toolbar from '$lib/components/Toolbar.svelte';
    import SidePanel from '$lib/components/SidePanel.svelte';
    import AddObjectMenu from '$lib/components/AddObjectMenu.svelte';
    import PaintToolbar from '$lib/components/PaintToolbar.svelte';
    import { bridge } from '$lib/bridge';
    import { onMount } from 'svelte';

    let renderStats = $state({
        fps: 0,
        frameTime: 0,
    });

    // Edit mode state
    let editMode = $state<'None' | 'Paint'>('None');

    // Add object menu state
    let showAddMenu = $state(false);
    let addMenuPosition = $state({ x: 0, y: 0 });

    function handleKeydown(e: KeyboardEvent) {
        // Shift+A opens the add object menu at cursor position
        if (e.shiftKey && e.key === 'A') {
            e.preventDefault();
            // Position menu at center of screen (we don't have cursor position here)
            addMenuPosition = {
                x: window.innerWidth / 2 - 75,
                y: window.innerHeight / 2 - 100,
            };
            showAddMenu = true;
        }
    }

    function handleMousemove(e: MouseEvent) {
        // Track mouse position for menu placement
        if (!showAddMenu) {
            addMenuPosition = { x: e.clientX, y: e.clientY };
        }
    }

    function handleAddMenuKeydown(e: KeyboardEvent) {
        // Shift+A opens the add object menu at last known cursor position
        // Note: key is lowercase 'a' because the Bevy keyboard forwarding uses lowercase letters
        if (e.shiftKey && e.key.toLowerCase() === 'a') {
            e.preventDefault();
            showAddMenu = true;
        }
    }

    onMount(() => {
        // Subscribe to messages from Bevy
        const unsubscribe = bridge.subscribe((msg) => {
            switch (msg.type) {
                case 'RenderStats':
                    renderStats = {
                        fps: msg.data.fps,
                        frameTime: msg.data.frame_time_ms,
                    };
                    break;
                case 'EditModeChanged':
                    editMode = msg.data.mode;
                    break;
            }
        });

        return unsubscribe;
    });
</script>

<svelte:window onkeydown={handleAddMenuKeydown} onmousemove={handleMousemove} />

<div class="app">
    <Toolbar {renderStats} />
    <SidePanel />
    <AddObjectMenu
        show={showAddMenu}
        position={addMenuPosition}
        onClose={() => (showAddMenu = false)}
    />
    <PaintToolbar visible={editMode === 'Paint'} />
</div>

<style>
    .app {
        width: 100vw;
        height: 100vh;
        /* Let events pass through to the canvas below */
        pointer-events: none;
    }

    /* Only enable pointer events on actual interactive elements, not wrapper divs */
    .app :global(button),
    .app :global(input),
    .app :global(select),
    .app :global(textarea),
    .app :global(a),
    .app :global(label),
    .app :global([role="button"]),
    .app :global(.interactive),
    .app :global(.toolbar),
    .app :global(.side-panel),
    .app :global(.add-menu-backdrop),
    .app :global(.paint-toolbar) {
        pointer-events: auto;
    }
</style>
