<script lang="ts">
    import Toolbar from '$lib/components/Toolbar.svelte';
    import SidePanel from '$lib/components/SidePanel.svelte';
    import AddObjectMenu from '$lib/components/AddObjectMenu.svelte';
    import BrushPanels from '$lib/components/BrushPanels.svelte';
    import { bridge } from '$lib/bridge';
    import { onMount } from 'svelte';

    let renderStats = $state({
        fps: 0,
        frameTime: 0,
    });

    let errorMessage = $state('');
    let projectNotice=$state('');

    // Edit mode state
    let editMode = $state<'None' | 'Paint' | 'MeshEdit' | 'Sculpt'>('None');

    // Add object menu state
    let showAddMenu = $state(false);
    let addMenuPosition = $state({ x: 0, y: 0 });

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
                case 'Error':
                    errorMessage = msg.data.message;
                    break;
                case 'ProjectOperationFinished':
                    projectNotice=msg.data.success?msg.data.message:'';
                    if(msg.data.success)errorMessage='';
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
    <Toolbar {renderStats} mode={editMode} />
    <div hidden={editMode === 'Paint' || editMode === 'Sculpt'}><SidePanel /></div>
    <AddObjectMenu
        show={showAddMenu}
        position={addMenuPosition}
        onClose={() => (showAddMenu = false)}
    />
    <BrushPanels mode={editMode} />
    {#if projectNotice}<div class="global-project-notice panel interactive" data-ui-region="project-notice" role="status"><span>{projectNotice}</span><button type="button" aria-label="Dismiss project notice" onclick={()=>projectNotice=''}>Dismiss</button></div>{/if}
    {#if errorMessage}
        <div class="global-error panel interactive" data-ui-region="global-error" role="alert">
            <span>{errorMessage}</span>
            <button type="button" aria-label="Dismiss error" onclick={() => (errorMessage = '')}>Dismiss</button>
        </div>
    {/if}
</div>

<style>
    .global-project-notice{position:fixed;bottom:18px;left:18px;z-index:290;display:flex;align-items:center;gap:16px;max-width:min(600px,calc(100vw - 36px));padding:12px 16px;background:#242a33;border:1px solid #6d849c;border-radius:6px;color:#e0eaf5;font:13px/1.5 system-ui,sans-serif}.global-project-notice button{padding:5px 8px;background:#334154;border:1px solid #6d849c;border-radius:4px;color:inherit}
    .global-error { position: fixed; bottom: 18px; left: 18px; z-index: 300; display: flex; align-items: center; gap: 16px; max-width: min(580px, calc(100vw - 36px)); padding: 14px 16px; border: 1px solid #bd8767; border-radius: 8px; background: #2f2423; color: #ffe1c7; font: 13px/1.5 system-ui, sans-serif; }
    .global-error button { flex-shrink: 0; color: inherit; background: #48312b; border: 1px solid #bd8767; border-radius: 4px; padding: 5px 8px; cursor: pointer; }
    .global-error button:focus-visible { outline: 2px solid #9abaff; outline-offset: 2px; }
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
