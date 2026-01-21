/**
 * IPC Bridge for communication between Svelte UI and Bevy backend
 *
 * Supports multiple modes:
 * - Native modes (capture/overlay/cef): Uses __PENTIMENTO_IPC__ injected by Rust
 * - Tauri mode: Uses CustomEvents for WASM <-> JS communication
 */

import type { BevyToUi, UiToBevy, LayoutInfo } from './types';

// Declare the IPC interface injected by Rust (native modes)
declare global {
    interface Window {
        __PENTIMENTO_IPC__: {
            postMessage: (msg: string) => void;
        };
        __PENTIMENTO_RECEIVE__: (msg: string) => void;
        __TAURI__?: unknown;
        __TAURI_INTERNALS__?: unknown;
    }
}

type MessageHandler = (msg: BevyToUi) => void;

/** Check if running in Tauri mode (Bevy WASM) */
function isTauriMode(): boolean {
    return '__TAURI__' in window || '__TAURI_INTERNALS__' in window;
}

class BevyBridge {
    private handlers: Set<MessageHandler> = new Set();
    private layoutDebounceTimer: ReturnType<typeof setTimeout> | null = null;
    private readonly tauriMode: boolean;

    constructor() {
        this.tauriMode = isTauriMode();

        if (this.tauriMode) {
            // Tauri mode: Listen for CustomEvents from Bevy WASM
            window.addEventListener('pentimento:bevy-to-ui', ((event: CustomEvent) => {
                try {
                    const msg: BevyToUi = JSON.parse(event.detail);
                    this.handlers.forEach(handler => handler(msg));
                } catch (e) {
                    console.error('Failed to parse Bevy WASM message:', e);
                }
            }) as EventListener);
            console.log('Pentimento bridge initialized in Tauri mode');
        } else {
            // Native modes: Set up message receiver (called from Rust)
            window.__PENTIMENTO_RECEIVE__ = (msgJson: string) => {
                try {
                    const msg: BevyToUi = JSON.parse(msgJson);
                    this.handlers.forEach(handler => handler(msg));
                } catch (e) {
                    console.error('Failed to parse IPC message:', e);
                }
            };
            console.log('Pentimento bridge initialized in native mode');
        }
    }

    /**
     * Subscribe to messages from Bevy
     * Returns an unsubscribe function
     */
    subscribe(handler: MessageHandler): () => void {
        this.handlers.add(handler);
        return () => this.handlers.delete(handler);
    }

    private send(msg: UiToBevy): void {
        if (this.tauriMode) {
            // Tauri mode: Send via CustomEvent to Bevy WASM
            window.dispatchEvent(new CustomEvent('pentimento:ui-to-bevy', {
                detail: JSON.stringify(msg)
            }));
        } else if (window.__PENTIMENTO_IPC__) {
            // Native modes: Use IPC injected by Rust
            window.__PENTIMENTO_IPC__.postMessage(JSON.stringify(msg));
        } else {
            console.warn('IPC not available - running outside Pentimento?');
        }
    }

    /**
     * Mark UI as dirty (needs re-capture)
     */
    markDirty(): void {
        this.send({ type: 'UiDirty' });
    }

    /**
     * Update layout info for input routing (debounced)
     */
    updateLayout(layout: LayoutInfo): void {
        if (this.layoutDebounceTimer) {
            clearTimeout(this.layoutDebounceTimer);
        }
        this.layoutDebounceTimer = setTimeout(() => {
            this.send({ type: 'LayoutUpdate', data: layout });
            this.layoutDebounceTimer = null;
        }, 16); // ~60fps max
    }

    // Camera controls
    cameraOrbit(deltaX: number, deltaY: number): void {
        this.send({
            type: 'CameraCommand',
            data: { Orbit: { delta_x: deltaX, delta_y: deltaY } }
        });
    }

    cameraPan(deltaX: number, deltaY: number): void {
        this.send({
            type: 'CameraCommand',
            data: { Pan: { delta_x: deltaX, delta_y: deltaY } }
        });
    }

    cameraZoom(delta: number): void {
        this.send({
            type: 'CameraCommand',
            data: { Zoom: { delta } }
        });
    }

    cameraReset(): void {
        this.send({
            type: 'CameraCommand',
            data: { Reset: null }
        });
    }

    // Object manipulation
    selectObjects(ids: string[]): void {
        this.send({
            type: 'ObjectCommand',
            data: { Select: { ids } }
        });
    }

    deleteObjects(ids: string[]): void {
        this.send({
            type: 'ObjectCommand',
            data: { Delete: { ids } }
        });
    }

    // Material editing
    updateMaterialProperty(materialId: string, property: string, value: unknown): void {
        this.send({
            type: 'MaterialCommand',
            data: {
                UpdateProperty: {
                    material_id: materialId,
                    property,
                    value
                }
            }
        });
    }

    // Diffusion
    startDiffusion(request: {
        taskId: string;
        prompt: string;
        negativePrompt?: string;
        width: number;
        height: number;
        steps: number;
        guidanceScale: number;
        seed?: number;
    }): void {
        this.send({
            type: 'StartDiffusion',
            data: {
                task_id: request.taskId,
                prompt: request.prompt,
                negative_prompt: request.negativePrompt ?? null,
                width: request.width,
                height: request.height,
                steps: request.steps,
                guidance_scale: request.guidanceScale,
                seed: request.seed ?? null,
                target_material_slot: null,
            }
        });
    }

    cancelDiffusion(taskId: string): void {
        this.send({ type: 'CancelDiffusion', data: { task_id: taskId } });
    }
}

export const bridge = new BevyBridge();

/**
 * Set up automatic dirty marking on DOM mutations
 */
export function setupAutoMarkDirty(): void {
    const observer = new MutationObserver(() => {
        bridge.markDirty();
    });

    observer.observe(document.body, {
        childList: true,
        subtree: true,
        attributes: true,
        characterData: true,
    });

    // Also mark dirty on window resize
    window.addEventListener('resize', () => {
        bridge.markDirty();
    });
}
