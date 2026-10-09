<script lang="ts">
    import { onMount } from 'svelte';
    import { bridge } from '$lib/bridge';
    import type { EditMode, SculptHistoryState } from '$lib/types';
    import type { ProjectState } from '$lib/types';
    import ProjectDialog from './ProjectDialog.svelte';

    interface Props {
        mode: EditMode;
        renderStats: {
            fps: number;
            frameTime: number;
        };
    }

    let { renderStats, mode }: Props = $props();
    let project=$state<ProjectState>({generation:"0",path:null,available:false,active:false,blocked:false,notice:null});
    let projectOperation=$state<'Save'|'Open'|'New'|null>(null);
    let paintUndo = $state(false);
    let paintRedo = $state(false);
    let sculptHistory = $state<SculptHistoryState>({ undo_strokes: 0, redo_strokes: 0, active: false, notice: null });
    let canUndo = $derived(mode === 'Paint' ? paintUndo : mode === 'Sculpt' && !sculptHistory.active && sculptHistory.undo_strokes > 0);
    let canRedo = $derived(mode === 'Paint' ? paintRedo : mode === 'Sculpt' && !sculptHistory.active && sculptHistory.redo_strokes > 0);

    onMount(() => bridge.subscribe(message => {
        if (message.type === 'PaintBrushStateChanged') {
            paintUndo = message.data.can_undo;
            paintRedo = message.data.can_redo;
        } else if (message.type === 'SculptHistoryChanged') {
            sculptHistory = message.data;
        } else if(message.type==='ProjectStateChanged'){
            project=message.data;
        }
    }));

    // Track which dropdown is open
    let openMenu = $state<string | null>(null);

    // Track selected tool
    let selectedTool = $state<string>('select');

    // Depth view toggle
    let depthViewEnabled = $state(false);
    let toolbarElement: HTMLElement | null = null;

    function handleResetCamera() {
        bridge.cameraReset();
    }

    function toggleMenu(menu: string) {
        if(menu==='file')bridge.projectCommand('GetState');
        openMenu = openMenu === menu ? null : menu;
    }

    function closeMenu() {
        openMenu = null;
    }

    function handleWindowClick(event: MouseEvent) {
        if (!openMenu || !toolbarElement || !(event.target instanceof Node)) {
            return;
        }

        if (!toolbarElement.contains(event.target)) {
            closeMenu();
        }
    }

    function handleWindowKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape' && openMenu) {
            event.preventDefault();
            closeMenu();
        }
    }

    function handleMenuAction(action: string) {
        if((action==='new'||action==='save'||action==='open'||action==='save-as') && project.available && !project.active){
            if(action==='save' && project.path && !project.blocked)bridge.projectCommand({Save:{path:project.path}});
            else projectOperation=action==='new'?'New':action==='open'?'Open':'Save';
        }
        if ((action === 'undo' && canUndo) || (action === 'redo' && canRedo)) {
            const command = action === 'undo' ? 'Undo' : 'Redo';
            if (mode === 'Paint') bridge.paintCommand(command);
            else if (mode === 'Sculpt') bridge.sculptCommand(command);
        }
        closeMenu();
    }

    function selectTool(tool: string) {
        selectedTool = tool;
    }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />
{#if projectOperation}<ProjectDialog operation={projectOperation} {project} onclose={()=>projectOperation=null}/>{/if}

<header bind:this={toolbarElement} class="toolbar panel">
    <div class="toolbar-left">
        <h1 class="title">Pentimento</h1>
        <nav class="nav">
            <div class="menu-container">
                <button
                    type="button"
                    class="nav-button"
                    class:active={openMenu === 'file'}
                    aria-haspopup="menu"
                    aria-expanded={openMenu === 'file'}
                    onclick={() => toggleMenu('file')}
                >
                    File
                </button>
                {#if openMenu === 'file'}
                    <div class="dropdown" role="menu" aria-label="File">
                        <button type="button" class="dropdown-item" role="menuitem" disabled={!project.available||project.active} onclick={() => handleMenuAction('new')}>New Project</button>
                        <button type="button" class="dropdown-item" role="menuitem" disabled={!project.available||project.active} onclick={() => handleMenuAction('open')}>Open...</button>
                        <button type="button" class="dropdown-item" role="menuitem" disabled={!project.available||project.active} onclick={() => handleMenuAction('save')}>Save</button>
                        <button type="button" class="dropdown-item" role="menuitem" disabled={!project.available||project.active} onclick={() => handleMenuAction('save-as')}>Save As...</button>
                        <div class="dropdown-divider"></div>
                        <button type="button" class="dropdown-item" role="menuitem" disabled>Export...</button>
                        {#if project.notice}<p class="project-notice" role="status">{project.notice}</p>{/if}
                    </div>
                {/if}
            </div>
            <div class="menu-container">
                <button
                    type="button"
                    class="nav-button"
                    class:active={openMenu === 'edit'}
                    aria-haspopup="menu"
                    aria-expanded={openMenu === 'edit'}
                    onclick={() => toggleMenu('edit')}
                >
                    Edit
                </button>
                {#if openMenu === 'edit'}
                    <div class="dropdown" role="menu" aria-label="Edit">
                        <button type="button" class="dropdown-item" role="menuitem" disabled={!canUndo} onclick={() => handleMenuAction('undo')}>Undo</button>
                        <button type="button" class="dropdown-item" role="menuitem" disabled={!canRedo} onclick={() => handleMenuAction('redo')}>Redo</button>
                        <div class="dropdown-divider"></div>
                        <button type="button" class="dropdown-item" role="menuitem" onclick={() => handleMenuAction('cut')}>Cut</button>
                        <button type="button" class="dropdown-item" role="menuitem" onclick={() => handleMenuAction('copy')}>Copy</button>
                        <button type="button" class="dropdown-item" role="menuitem" onclick={() => handleMenuAction('paste')}>Paste</button>
                    </div>
                {/if}
            </div>
            <div class="menu-container">
                <button
                    type="button"
                    class="nav-button"
                    class:active={openMenu === 'view'}
                    aria-haspopup="menu"
                    aria-expanded={openMenu === 'view'}
                    onclick={() => toggleMenu('view')}
                >
                    View
                </button>
                {#if openMenu === 'view'}
                    <div class="dropdown" role="menu" aria-label="View">
                        <button type="button" class="dropdown-item" role="menuitem" onclick={() => handleMenuAction('zoom-in')}>Zoom In</button>
                        <button type="button" class="dropdown-item" role="menuitem" onclick={() => handleMenuAction('zoom-out')}>Zoom Out</button>
                        <button type="button" class="dropdown-item" role="menuitem" onclick={() => handleMenuAction('fit')}>Fit to Window</button>
                    </div>
                {/if}
            </div>
        </nav>
    </div>

    <div class="toolbar-center">
        <div class="tool-group">
            <button
                type="button"
                class="tool-button"
                class:selected={selectedTool === 'select'}
                title="Select"
                aria-label="Select tool"
                aria-pressed={selectedTool === 'select'}
                onclick={() => selectTool('select')}
            >
                <span class="icon">↖</span>
            </button>
            <button
                type="button"
                class="tool-button"
                class:selected={selectedTool === 'move'}
                title="Move"
                aria-label="Move tool"
                aria-pressed={selectedTool === 'move'}
                onclick={() => selectTool('move')}
            >
                <span class="icon">✥</span>
            </button>
            <button
                type="button"
                class="tool-button"
                class:selected={selectedTool === 'rotate'}
                title="Rotate"
                aria-label="Rotate tool"
                aria-pressed={selectedTool === 'rotate'}
                onclick={() => selectTool('rotate')}
            >
                <span class="icon">↻</span>
            </button>
            <button
                type="button"
                class="tool-button"
                class:selected={selectedTool === 'scale'}
                title="Scale"
                aria-label="Scale tool"
                aria-pressed={selectedTool === 'scale'}
                onclick={() => selectTool('scale')}
            >
                <span class="icon">⤢</span>
            </button>
        </div>
    </div>

    <div class="toolbar-right">
        <button
            type="button"
            class="tool-button"
            class:selected={depthViewEnabled}
            title="Depth View"
            aria-label="Toggle depth view"
            aria-pressed={depthViewEnabled}
            onclick={() => {
                depthViewEnabled = !depthViewEnabled;
                bridge.setDepthView(depthViewEnabled);
            }}
        >
            <span class="icon">D</span>
        </button>
        <button type="button" class="nav-button" onclick={handleResetCamera}>Reset Camera</button>
        <div class="stats">
            <span class="stat">{renderStats.fps.toFixed(0)} FPS</span>
            <span class="stat">{renderStats.frameTime.toFixed(1)}ms</span>
        </div>
    </div>
</header>

<style>
    .toolbar {
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        height: 48px;
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0 16px;
        z-index: 100;
    }

    .toolbar-left,
    .toolbar-center,
    .toolbar-right {
        display: flex;
        align-items: center;
        gap: 16px;
    }

    .title {
        font-size: 16px;
        font-weight: 600;
        color: white;
        margin: 0;
    }

    .nav {
        display: flex;
        gap: 4px;
    }

    .nav-button {
        background: transparent;
        border: none;
        color: rgba(255, 255, 255, 0.8);
        padding: 6px 12px;
        border-radius: 4px;
        font-size: 13px;
        cursor: pointer;
        transition: background 0.15s;
    }

    .nav-button:hover,
    .nav-button.active {
        background: rgba(255, 255, 255, 0.1);
        color: white;
    }

    .nav-button:focus-visible,
    .dropdown-item:focus-visible,
    .tool-button:focus-visible {
        outline: 2px solid rgba(100, 150, 255, 0.9);
        outline-offset: 2px;
    }

    .menu-container {
        position: relative;
    }

    .dropdown {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: 4px;
        min-width: 160px;
        background: rgba(30, 30, 30, 0.95);
        backdrop-filter: blur(10px);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 6px;
        padding: 4px;
        z-index: 200;
    }

    .dropdown-item {
        display: block;
        width: 100%;
        padding: 8px 12px;
        background: transparent;
        border: none;
        color: rgba(255, 255, 255, 0.9);
        font-size: 13px;
        text-align: left;
        cursor: pointer;
        border-radius: 4px;
        transition: background 0.1s;
    }

    .dropdown-item:hover {
        background: rgba(255, 255, 255, 0.1);
    }

    .dropdown-item:disabled {
        opacity: 0.45;
        cursor: default;
        background: transparent;
    }

    .dropdown-divider {
        height: 1px;
        background: rgba(255, 255, 255, 0.1);
        margin: 4px 0;
    }

    .tool-group {
        display: flex;
        gap: 2px;
        background: rgba(0, 0, 0, 0.3);
        padding: 4px;
        border-radius: 6px;
    }

    .tool-button {
        background: transparent;
        border: none;
        color: rgba(255, 255, 255, 0.7);
        width: 32px;
        height: 32px;
        border-radius: 4px;
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        transition: all 0.15s;
    }

    .tool-button:hover {
        background: rgba(255, 255, 255, 0.15);
        color: white;
    }

    .tool-button.selected {
        background: rgba(100, 150, 255, 0.3);
        color: white;
    }

    .icon {
        font-size: 16px;
    }

    .stats {
        display: flex;
        gap: 12px;
        font-size: 12px;
        color: rgba(255, 255, 255, 0.5);
        font-family: monospace;
    }

    .stat {
        min-width: 60px;
    }
</style>
