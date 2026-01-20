<script lang="ts">
    import { bridge } from '$lib/bridge';

    interface Props {
        renderStats: {
            fps: number;
            frameTime: number;
        };
    }

    let { renderStats }: Props = $props();

    function handleResetCamera() {
        bridge.cameraReset();
    }
</script>

<header class="toolbar panel">
    <div class="toolbar-left">
        <h1 class="title">Pentimento</h1>
        <nav class="nav">
            <button class="nav-button">File</button>
            <button class="nav-button">Edit</button>
            <button class="nav-button">View</button>
        </nav>
    </div>

    <div class="toolbar-center">
        <div class="tool-group">
            <button class="tool-button" title="Select">
                <span class="icon">↖</span>
            </button>
            <button class="tool-button" title="Move">
                <span class="icon">✥</span>
            </button>
            <button class="tool-button" title="Rotate">
                <span class="icon">↻</span>
            </button>
            <button class="tool-button" title="Scale">
                <span class="icon">⤢</span>
            </button>
        </div>
    </div>

    <div class="toolbar-right">
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

    .nav-button:hover {
        background: rgba(255, 255, 255, 0.1);
        color: white;
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
