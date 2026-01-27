<script lang="ts">
    import { bridge } from '$lib/bridge';

    interface Props {
        show: boolean;
        position: { x: number; y: number };
        onClose: () => void;
    }

    let { show, position, onClose }: Props = $props();

    const primitives = [
        { type: 'Cube', label: 'Cube' },
        { type: 'Sphere', label: 'Sphere' },
        { type: 'Cylinder', label: 'Cylinder' },
        { type: 'Plane', label: 'Plane' },
        { type: 'Torus', label: 'Torus' },
        { type: 'Cone', label: 'Cone' },
        { type: 'Capsule', label: 'Capsule' },
    ];

    function addObject(type: string) {
        bridge.addObject({ primitiveType: type });
        onClose();
    }

    function addPaintCanvas() {
        bridge.addPaintCanvas();
        onClose();
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape') {
            onClose();
        }
    }

    function handleBackdropClick() {
        onClose();
    }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if show}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="add-menu-backdrop" role="presentation" onclick={handleBackdropClick}>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
            class="add-menu panel"
            role="dialog"
            aria-label="Add Object Menu"
            tabindex="-1"
            style="left: {position.x}px; top: {position.y}px;"
            onclick={(e) => e.stopPropagation()}
        >
            <h3 class="menu-title">Add Object</h3>
            <div class="menu-items">
                {#each primitives as prim}
                    <button class="menu-item" onclick={() => addObject(prim.type)}>
                        {prim.label}
                    </button>
                {/each}
                <div class="menu-divider"></div>
                <button class="menu-item" onclick={addPaintCanvas}>
                    Paint
                </button>
            </div>
        </div>
    </div>
{/if}

<style>
    .add-menu-backdrop {
        position: fixed;
        inset: 0;
        z-index: 300;
    }

    .add-menu {
        position: absolute;
        min-width: 150px;
        background: rgba(30, 30, 30, 0.98);
        backdrop-filter: blur(10px);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 8px;
        padding: 8px;
    }

    .menu-title {
        font-size: 11px;
        text-transform: uppercase;
        color: rgba(255, 255, 255, 0.5);
        margin: 0 0 8px 8px;
        letter-spacing: 0.05em;
    }

    .menu-items {
        display: flex;
        flex-direction: column;
    }

    .menu-item {
        display: flex;
        align-items: center;
        gap: 8px;
        width: 100%;
        padding: 8px 12px;
        background: transparent;
        border: none;
        color: rgba(255, 255, 255, 0.9);
        font-size: 13px;
        text-align: left;
        cursor: pointer;
        border-radius: 4px;
    }

    .menu-item:hover {
        background: rgba(255, 255, 255, 0.1);
    }

    .menu-divider {
        height: 1px;
        background: rgba(255, 255, 255, 0.1);
        margin: 8px 0;
    }
</style>
