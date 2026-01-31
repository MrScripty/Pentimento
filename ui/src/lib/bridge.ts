/**
 * IPC Bridge for communication between Svelte UI and Bevy backend
 *
 * Supports multiple modes:
 * - Native modes (capture/overlay/cef): Uses __PENTIMENTO_IPC__ injected by Rust
 * - WASM modes (Tauri/Electron): Uses CustomEvents for WASM <-> JS communication
 */

import type { BevyToUi, UiToBevy, LayoutInfo } from './types';

// Declare the IPC interface injected by Rust (native modes)
declare global {
    interface Window {
        __PENTIMENTO_IPC__?: {
            postMessage: (msg: string) => void;
        };
        __PENTIMENTO_RECEIVE__?: (msg: string) => void;
        ipc?: {
            postMessage: (msg: string) => void;
        };
        __TAURI__?: unknown;
        __TAURI_INTERNALS__?: unknown;
        __ELECTRON__?: boolean;
    }
}

type MessageHandler = (msg: BevyToUi) => void;

/** Check if running in WASM mode (Tauri or Electron with Bevy WASM) */
function isWasmMode(): boolean {
    return '__TAURI__' in window ||
           '__TAURI_INTERNALS__' in window ||
           '__ELECTRON__' in window;
}

function getNativeIpc(): { postMessage: (msg: string) => void } | null {
    if (window.__PENTIMENTO_IPC__) {
        return window.__PENTIMENTO_IPC__;
    }
    if (window.ipc) {
        return window.ipc;
    }
    return null;
}

class BevyBridge {
    private handlers: Set<MessageHandler> = new Set();
    private layoutDebounceTimer: ReturnType<typeof setTimeout> | null = null;
    private readonly wasmMode: boolean;

    constructor() {
        this.wasmMode = isWasmMode();

        if (this.wasmMode) {
            // WASM mode (Tauri/Electron): Listen for CustomEvents from Bevy WASM
            window.addEventListener('pentimento:bevy-to-ui', ((event: CustomEvent) => {
                try {
                    const msg: BevyToUi = JSON.parse(event.detail);
                    this.handlers.forEach(handler => handler(msg));
                } catch (e) {
                    console.error('Failed to parse Bevy WASM message:', e);
                }
            }) as EventListener);
            const runtime = '__ELECTRON__' in window ? 'Electron' : 'Tauri';
            console.log(`Pentimento bridge initialized in ${runtime} WASM mode`);
        } else {
            const ipc = getNativeIpc();
            if (!window.__PENTIMENTO_IPC__ && ipc) {
                window.__PENTIMENTO_IPC__ = ipc;
            }
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
        if (this.wasmMode) {
            // WASM mode (Tauri/Electron): Send via CustomEvent to Bevy WASM
            window.dispatchEvent(new CustomEvent('pentimento:ui-to-bevy', {
                detail: JSON.stringify(msg)
            }));
        } else {
            // Native modes: Use IPC injected by Rust
            const ipc = getNativeIpc();
            if (ipc) {
                ipc.postMessage(JSON.stringify(msg));
            } else {
                console.warn('IPC not available - running outside Pentimento?');
            }
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

    // Lighting controls
    updateLighting(settings: {
        sunDirection?: [number, number, number];
        sunColor?: [number, number, number];
        sunIntensity?: number;
        ambientColor?: [number, number, number];
        ambientIntensity?: number;
        timeOfDay?: number;
        cloudiness?: number;
        useTimeOfDay?: boolean;
        moonPhase?: number;
        azimuthAngle?: number;
        pollution?: number;
    }): void {
        this.send({
            type: 'UpdateLighting',
            data: {
                sun_direction: settings.sunDirection ?? [-0.5, -0.7, -0.5],
                sun_color: settings.sunColor ?? [1.0, 0.98, 0.95],
                sun_intensity: settings.sunIntensity ?? 10000.0,
                ambient_color: settings.ambientColor ?? [0.6, 0.7, 1.0],
                ambient_intensity: settings.ambientIntensity ?? 500.0,
                time_of_day: settings.timeOfDay ?? 12.0,
                cloudiness: settings.cloudiness ?? 0.0,
                use_time_of_day: settings.useTimeOfDay ?? true,
                moon_phase: settings.moonPhase ?? 0.5,
                azimuth_angle: settings.azimuthAngle ?? 0.0,
                pollution: settings.pollution ?? 0.0,
            }
        });
    }

    // Ambient occlusion controls
    updateAmbientOcclusion(settings: {
        enabled: boolean;
        qualityLevel: number;
        constantObjectThickness: number;
    }): void {
        this.send({
            type: 'UpdateAmbientOcclusion',
            data: {
                enabled: settings.enabled,
                quality_level: settings.qualityLevel,
                constant_object_thickness: settings.constantObjectThickness,
            }
        });
    }

    // Add object to scene
    addObject(request: {
        primitiveType: string;
        position?: [number, number, number];
        name?: string;
    }): void {
        this.send({
            type: 'AddObject',
            data: {
                primitive_type: request.primitiveType,
                position: request.position ?? null,
                name: request.name ?? null,
            }
        });
    }

    // Add paint canvas
    addPaintCanvas(options?: { width?: number; height?: number }): void {
        this.send({
            type: 'AddPaintCanvas',
            data: {
                width: options?.width ?? null,
                height: options?.height ?? null,
            }
        });
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
