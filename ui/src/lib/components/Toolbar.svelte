<script lang="ts">
    import { bridge } from '$lib/bridge';

    interface Props {
        renderStats: {
            fps: number;
            frameTime: number;
        };
    }

    let { renderStats }: Props = $props();

    // Track which dropdown is open
    let openMenu = $state<string | null>(null);

    // Track selected tool
    let selectedTool = $state<string>('select');

    // Depth view toggle
    let depthViewEnabled = $state(false);

    function handleResetCamera() {
        bridge.cameraReset();
    }

    function toggleMenu(menu: string) {
        openMenu = openMenu === menu ? null : menu;
    }

    function closeMenu() {
        openMenu = null;
    }

    function handleMenuAction(action: string) {
        console.log('Menu action:', action);
        closeMenu();
    }

    function selectTool(tool: string) {
        selectedTool = tool;
        console.log('Selected tool:', tool);
    }
</script>

<header class="toolbar panel">
    <div class="toolbar-left">
        <h1 class="title">Pentimento</h1>
        <nav class="nav">
            <div class="menu-container">
                <button class="nav-button" class:active={openMenu === 'file'} onclick={() => toggleMenu('file')}>File</button>
                {#if openMenu === 'file'}
                    <div class="dropdown">
                        <button class="dropdown-item" onclick={() => handleMenuAction('new')}>New Project</button>
                        <button class="dropdown-item" onclick={() => handleMenuAction('open')}>Open...</button>
                        <button class="dropdown-item" onclick={() => handleMenuAction('save')}>Save</button>
                        <div class="dropdown-divider"></div>
                        <button class="dropdown-item" onclick={() => handleMenuAction('export')}>Export...</button>
                    </div>
                {/if}
            </div>
            <div class="menu-container">
                <button class="nav-button" class:active={openMenu === 'edit'} onclick={() => toggleMenu('edit')}>Edit</button>
                {#if openMenu === 'edit'}
                    <div class="dropdown">
                        <button class="dropdown-item" onclick={() => handleMenuAction('undo')}>Undo</button>
                        <button class="dropdown-item" onclick={() => handleMenuAction('redo')}>Redo</button>
                        <div class="dropdown-divider"></div>
                        <button class="dropdown-item" onclick={() => handleMenuAction('cut')}>Cut</button>
                        <button class="dropdown-item" onclick={() => handleMenuAction('copy')}>Copy</button>
                        <button class="dropdown-item" onclick={() => handleMenuAction('paste')}>Paste</button>
                    </div>
                {/if}
            </div>
            <div class="menu-container">
                <button class="nav-button" class:active={openMenu === 'view'} onclick={() => toggleMenu('view')}>View</button>
                {#if openMenu === 'view'}
                    <div class="dropdown">
                        <button class="dropdown-item" onclick={() => handleMenuAction('zoom-in')}>Zoom In</button>
                        <button class="dropdown-item" onclick={() => handleMenuAction('zoom-out')}>Zoom Out</button>
                        <button class="dropdown-item" onclick={() => handleMenuAction('fit')}>Fit to Window</button>
                    </div>
                {/if}
            </div>
        </nav>
    </div>

    <div class="toolbar-center">
        <div class="tool-group">
            <button class="tool-button" class:selected={selectedTool === 'select'} title="Select" onclick={() => selectTool('select')}>
                <span class="icon">↖</span>
            </button>
            <button class="tool-button" class:selected={selectedTool === 'move'} title="Move" onclick={() => selectTool('move')}>
                <span class="icon">✥</span>
            </button>
            <button class="tool-button" class:selected={selectedTool === 'rotate'} title="Rotate" onclick={() => selectTool('rotate')}>
                <span class="icon">↻</span>
            </button>
            <button class="tool-button" class:selected={selectedTool === 'scale'} title="Scale" onclick={() => selectTool('scale')}>
                <span class="icon">⤢</span>
            </button>
        </div>
    </div>

    <div class="toolbar-right">
        <button
            class="tool-button"
            class:selected={depthViewEnabled}
            title="Depth View"
            onclick={() => {
                depthViewEnabled = !depthViewEnabled;
                bridge.setDepthView(depthViewEnabled);
            }}
        >
            <span class="icon">D</span>
        </button>
        <button class="nav-button" onclick={handleResetCamera}>Reset Camera</button>
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
