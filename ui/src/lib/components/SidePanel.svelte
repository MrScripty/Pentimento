<script lang="ts">
    import { bridge } from '$lib/bridge';
    import { onMount } from 'svelte';

    let selectedObjects = $state<string[]>([]);
    let materialProps = $state({
        baseColor: [0.8, 0.2, 0.2, 1.0],
        metallic: 0.5,
        roughness: 0.3,
    });

    // Lighting settings
    let lightingSettings = $state({
        timeOfDay: 12.0,
        cloudiness: 0.0,
        sunIntensity: 10000.0,
        ambientIntensity: 500.0,
        moonPhase: 50,      // 0-100%
        azimuthAngle: 0,    // 0-360 degrees
        pollution: 0,       // 0-100%
    });

    // Ambient occlusion settings
    let aoSettings = $state({
        enabled: false,
        qualityLevel: 2,
        constantObjectThickness: 0.25,
    });

    // Check if running in WASM mode (SSAO not supported)
    const isWasm = typeof window !== 'undefined' &&
        ('__TAURI__' in window || '__TAURI_INTERNALS__' in window || '__ELECTRON__' in window);

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

    function sendLightingUpdate() {
        bridge.updateLighting({
            timeOfDay: lightingSettings.timeOfDay,
            cloudiness: lightingSettings.cloudiness,
            sunIntensity: lightingSettings.sunIntensity,
            ambientIntensity: lightingSettings.ambientIntensity,
            useTimeOfDay: true,
            moonPhase: lightingSettings.moonPhase / 100,
            azimuthAngle: lightingSettings.azimuthAngle,
            pollution: lightingSettings.pollution / 100,
        });
    }

    function handleTimeOfDayChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber;
        lightingSettings.timeOfDay = value;
        sendLightingUpdate();
    }

    function handleCloudinessChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber / 100;
        lightingSettings.cloudiness = value;
        sendLightingUpdate();
    }

    function handleMoonPhaseChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber;
        lightingSettings.moonPhase = value;
        sendLightingUpdate();
    }

    function handleAzimuthChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber;
        lightingSettings.azimuthAngle = value;
        sendLightingUpdate();
    }

    function handlePollutionChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber;
        lightingSettings.pollution = value;
        sendLightingUpdate();
    }

    function handleAoEnabledChange(e: Event) {
        const checked = (e.target as HTMLInputElement).checked;
        aoSettings.enabled = checked;
        bridge.updateAmbientOcclusion({
            enabled: checked,
            qualityLevel: aoSettings.qualityLevel,
            constantObjectThickness: aoSettings.constantObjectThickness,
        });
    }

    function handleAoQualityChange(e: Event) {
        const value = parseInt((e.target as HTMLSelectElement).value);
        aoSettings.qualityLevel = value;
        bridge.updateAmbientOcclusion({
            enabled: aoSettings.enabled,
            qualityLevel: value,
            constantObjectThickness: aoSettings.constantObjectThickness,
        });
    }

    function handleAoIntensityChange(e: Event) {
        const value = (e.target as HTMLInputElement).valueAsNumber;
        aoSettings.constantObjectThickness = value;
        bridge.updateAmbientOcclusion({
            enabled: aoSettings.enabled,
            qualityLevel: aoSettings.qualityLevel,
            constantObjectThickness: value,
        });
    }

    function formatTime(hours: number): string {
        const h = Math.floor(hours);
        const m = Math.floor((hours - h) * 60);
        return `${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}`;
    }

    function getMoonPhaseLabel(phase: number): string {
        if (phase < 10) return 'New';
        if (phase < 40) return 'Crescent';
        if (phase < 60) return 'Half';
        if (phase < 90) return 'Gibbous';
        return 'Full';
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
                    <label class="property-label" for="metallic-slider">Metallic</label>
                    <input
                        id="metallic-slider"
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
                    <label class="property-label" for="roughness-slider">Roughness</label>
                    <input
                        id="roughness-slider"
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
        <h2 class="section-title">Lighting</h2>

        <div class="property-group">
            <h3 class="group-title">Sun / Sky</h3>

            <div class="property">
                <label class="property-label" for="time-of-day-slider">Time of Day</label>
                <input
                    id="time-of-day-slider"
                    type="range"
                    min="0"
                    max="24"
                    step="0.1"
                    value={lightingSettings.timeOfDay}
                    oninput={handleTimeOfDayChange}
                    class="slider"
                />
                <span class="property-value">{formatTime(lightingSettings.timeOfDay)}</span>
            </div>

            <div class="property">
                <label class="property-label" for="cloudiness-slider">Cloudiness</label>
                <input
                    id="cloudiness-slider"
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    value={lightingSettings.cloudiness * 100}
                    oninput={handleCloudinessChange}
                    class="slider"
                />
                <span class="property-value">{(lightingSettings.cloudiness * 100).toFixed(0)}%</span>
            </div>

            <div class="property">
                <label class="property-label" for="azimuth-slider">Sun Angle</label>
                <input
                    id="azimuth-slider"
                    type="range"
                    min="0"
                    max="360"
                    step="1"
                    value={lightingSettings.azimuthAngle}
                    oninput={handleAzimuthChange}
                    class="slider"
                />
                <span class="property-value">{lightingSettings.azimuthAngle}°</span>
            </div>

            <div class="property">
                <label class="property-label" for="pollution-slider">Pollution</label>
                <input
                    id="pollution-slider"
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    value={lightingSettings.pollution}
                    oninput={handlePollutionChange}
                    class="slider"
                />
                <span class="property-value">{lightingSettings.pollution}%</span>
            </div>
        </div>

        <div class="property-group">
            <h3 class="group-title">Moon</h3>

            <div class="property">
                <label class="property-label" for="moon-phase-slider">Moon Phase</label>
                <input
                    id="moon-phase-slider"
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    value={lightingSettings.moonPhase}
                    oninput={handleMoonPhaseChange}
                    class="slider"
                />
                <span class="property-value">{getMoonPhaseLabel(lightingSettings.moonPhase)}</span>
            </div>
        </div>
    </section>

    <section class="section">
        <h2 class="section-title">Ambient Occlusion</h2>

        {#if isWasm}
            <p class="placeholder disabled-notice" title="SSAO is not supported in WebGL2/WASM mode">
                Not supported in browser
            </p>
        {:else}
            <div class="property-group">
                <div class="property checkbox-property">
                    <label class="property-label" for="ssao-checkbox">Enable SSAO</label>
                    <input
                        id="ssao-checkbox"
                        type="checkbox"
                        checked={aoSettings.enabled}
                        onchange={handleAoEnabledChange}
                        class="checkbox"
                    />
                    <span></span>
                </div>

                {#if aoSettings.enabled}
                    <div class="property">
                        <label class="property-label" for="ao-quality-select">Quality</label>
                        <select
                            id="ao-quality-select"
                            value={aoSettings.qualityLevel}
                            onchange={handleAoQualityChange}
                            class="select"
                        >
                            <option value={0}>Low</option>
                            <option value={1}>Medium</option>
                            <option value={2}>High</option>
                            <option value={3}>Ultra</option>
                        </select>
                        <span></span>
                    </div>

                    <div class="property">
                        <label class="property-label" for="ao-intensity-slider">Intensity</label>
                        <input
                            id="ao-intensity-slider"
                            type="range"
                            min="0.0625"
                            max="4"
                            step="0.0625"
                            value={aoSettings.constantObjectThickness}
                            oninput={handleAoIntensityChange}
                            class="slider"
                        />
                        <span class="property-value">{aoSettings.constantObjectThickness.toFixed(2)}</span>
                    </div>
                {/if}
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

    .disabled-notice {
        font-style: italic;
        cursor: help;
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

    .checkbox-property {
        grid-template-columns: 80px auto 1fr;
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

    .checkbox {
        width: 16px;
        height: 16px;
        cursor: pointer;
    }

    .select {
        width: 100%;
        padding: 4px 8px;
        background: rgba(255, 255, 255, 0.1);
        border: 1px solid rgba(255, 255, 255, 0.2);
        border-radius: 4px;
        color: white;
        font-size: 12px;
        cursor: pointer;
    }

    .select option {
        background: #2a2a2a;
    }

    .property-value {
        font-size: 11px;
        font-family: monospace;
        color: rgba(255, 255, 255, 0.5);
        text-align: right;
    }
</style>
