<script lang="ts">
    import { onDestroy, tick } from 'svelte';
    import { bridge } from '$lib/bridge';

    interface Props {
        show: boolean;
        position: { x: number; y: number };
        onClose: () => void;
    }

    let { show, position, onClose }: Props = $props();
    let menuElement = $state<HTMLDivElement | null>(null);
    let restoreFocusTo = $state<HTMLElement | null>(null);
    let wasOpen = false;
    const menuTitleId = 'add-object-menu-title';

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

    function getFocusableElements(): HTMLElement[] {
        if (!menuElement) {
            return [];
        }

        return Array.from(
            menuElement.querySelectorAll<HTMLElement>(
                'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
            )
        ).filter((element) => !element.hasAttribute('disabled'));
    }

    function focusDialogEntry() {
        const [firstFocusable] = getFocusableElements();
        (firstFocusable ?? menuElement)?.focus();
    }

    function restoreFocus() {
        if (restoreFocusTo?.isConnected) {
            restoreFocusTo.focus();
        }
        restoreFocusTo = null;
    }

    function handleDialogKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
            return;
        }

        if (e.key !== 'Tab') {
            return;
        }

        const focusable = getFocusableElements();
        if (focusable.length === 0) {
            e.preventDefault();
            menuElement?.focus();
            return;
        }

        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        const activeElement = document.activeElement;

        if (e.shiftKey && (activeElement === first || activeElement === menuElement)) {
            e.preventDefault();
            last.focus();
        } else if (!e.shiftKey && activeElement === last) {
            e.preventDefault();
            first.focus();
        }
    }

    $effect(() => {
        if (show && !wasOpen) {
            restoreFocusTo = document.activeElement instanceof HTMLElement ? document.activeElement : null;
            tick().then(focusDialogEntry);
        } else if (!show && wasOpen) {
            tick().then(restoreFocus);
        }

        wasOpen = show;
    });

    onDestroy(() => {
        restoreFocus();
    });
</script>

{#if show}
    <div class="add-menu-backdrop" role="presentation">
        <button
            type="button"
            class="add-menu-backdrop-dismiss"
            aria-label="Close add object menu"
            onclick={onClose}
        ></button>
        <div
            bind:this={menuElement}
            class="add-menu panel"
            role="dialog"
            aria-modal="true"
            aria-labelledby={menuTitleId}
            tabindex="-1"
            style="left: {position.x}px; top: {position.y}px;"
            onkeydown={handleDialogKeydown}
        >
            <h3 class="menu-title" id={menuTitleId}>Add Object</h3>
            <div class="menu-items">
                {#each primitives as prim}
                    <button type="button" class="menu-item" onclick={() => addObject(prim.type)}>
                        {prim.label}
                    </button>
                {/each}
                <div class="menu-divider"></div>
                <button type="button" class="menu-item" onclick={addPaintCanvas}>
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

    .add-menu-backdrop-dismiss {
        position: absolute;
        inset: 0;
        border: none;
        padding: 0;
        background: transparent;
    }

    .add-menu {
        position: absolute;
        z-index: 1;
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

    .menu-item:focus-visible,
    .add-menu-backdrop-dismiss:focus-visible {
        outline: 2px solid rgba(100, 150, 255, 0.9);
        outline-offset: 2px;
    }

    .menu-divider {
        height: 1px;
        background: rgba(255, 255, 255, 0.1);
        margin: 8px 0;
    }
</style>
