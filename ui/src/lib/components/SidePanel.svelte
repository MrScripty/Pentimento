<script lang="ts">
    import { bridge } from '$lib/bridge';
    import { onMount } from 'svelte';

    let selectedObjects = $state<string[]>([]);
    let materialProps = $state({
        baseColor: [0.8, 0.2, 0.2, 1.0],
        metallic: 0.5,
        roughness: 0.3,
    });

    onMount(() => {
        const unsubscribe = bridge.subscribe((msg) => {
            if (msg.type === 'SelectionChanged') {
                selectedObjects = msg.data.selected_ids;
            } else if (msg.type === 'MaterialUpdated') {
                materialProps = {
                    baseColor: msg.data.properties.base_color,
                    metallic: msg.data.properties.metallic,
                    roughness: msg.data.properties.roughness,
                };
            }
        });

        return unsubscribe;
    });

    function handleMetallicChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber;
        materialProps.metallic = value;
        if (selectedObjects.length > 0) {
            bridge.updateMaterialProperty(selectedObjects[0], 'metallic', value);
        }
    }

    function handleRoughnessChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber;
        materialProps.roughness = value;
        if (selectedObjects.length > 0) {
            bridge.updateMaterialProperty(selectedObjects[0], 'roughness', value);
        }
    }
</script>

<aside class="side-panel panel">
    <section class="section">
        <h2 class="section-title">Properties</h2>

        {#if selectedObjects.length === 0}
            <p class="placeholder">Select an object to view properties</p>
        {:else}
            <div class="property-group">
                <h3 class="group-title">Material</h3>

                <div class="property">
                    <label class="property-label">Metallic</label>
                    <input
                        type="range"
                        min="0"
                        max="1"
                        step="0.01"
                        value={materialProps.metallic}
                        oninput={handleMetallicChange}
                        class="slider"
                    />
                    <span class="property-value">{materialProps.metallic.toFixed(2)}</span>
                </div>

                <div class="property">
                    <label class="property-label">Roughness</label>
                    <input
                        type="range"
                        min="0"
                        max="1"
                        step="0.01"
                        value={materialProps.roughness}
                        oninput={handleRoughnessChange}
                        class="slider"
                    />
                    <span class="property-value">{materialProps.roughness.toFixed(2)}</span>
                </div>
            </div>
        {/if}
    </section>

    <section class="section">
        <h2 class="section-title">Diffusion</h2>
        <p class="placeholder">Connect to a diffusion server to generate textures</p>
    </section>
</aside>

<style>
    .side-panel {
        position: fixed;
        top: 56px;
        right: 8px;
        bottom: 8px;
        width: 300px;
        border-radius: 8px;
        overflow-y: auto;
        z-index: 50;
    }

    .section {
        padding: 16px;
        border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    }

    .section:last-child {
        border-bottom: none;
    }

    .section-title {
        font-size: 12px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: rgba(255, 255, 255, 0.5);
        margin: 0 0 12px 0;
    }

    .placeholder {
        font-size: 13px;
        color: rgba(255, 255, 255, 0.4);
        margin: 0;
    }

    .property-group {
        margin-bottom: 16px;
    }

    .group-title {
        font-size: 13px;
        font-weight: 500;
        color: rgba(255, 255, 255, 0.8);
        margin: 0 0 12px 0;
    }

    .property {
        display: grid;
        grid-template-columns: 80px 1fr 40px;
        align-items: center;
        gap: 8px;
        margin-bottom: 8px;
    }

    .property-label {
        font-size: 12px;
        color: rgba(255, 255, 255, 0.6);
    }

    .slider {
        width: 100%;
        height: 4px;
        background: rgba(255, 255, 255, 0.1);
        border-radius: 2px;
        appearance: none;
        cursor: pointer;
    }

    .slider::-webkit-slider-thumb {
        appearance: none;
        width: 12px;
        height: 12px;
        background: white;
        border-radius: 50%;
        cursor: pointer;
    }

    .property-value {
        font-size: 11px;
        font-family: monospace;
        color: rgba(255, 255, 255, 0.5);
        text-align: right;
    }
</style>
