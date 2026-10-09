<script lang="ts">
    import { bridge } from '$lib/bridge';
    import { colorToHex as toHex, hexToColor } from '$lib/brush-values';
    import type { PaintTargetState, ColorSampleSource, PaintColorSamplingState, PaintBrushSettings, PaintBrushPresetInfo, SavedBrushPresetsState } from '$lib/types';
    import SavedBrushPresets from './SavedBrushPresets.svelte';
    import BrushControl from './BrushControl.svelte';
    import UvLayerPanel from './UvLayerPanel.svelte';
    interface Props { target: PaintTargetState; settings: PaintBrushSettings; presets: PaintBrushPresetInfo[]; canUndo: boolean; canRedo: boolean; sourceVisible: boolean; liveProjection: boolean; saved: SavedBrushPresetsState; sampling: PaintColorSamplingState }
    let { target, settings, presets, canUndo, canRedo, sourceVisible, liveProjection, saved, sampling }: Props = $props();
    let direct = $derived(target.mode === 'DirectUv');
    let brushLocked = $derived(direct && target.active);
    let sampleSource = $state<ColorSampleSource>('VisibleLayers');
    $effect(() => { if (sampling.active) { sampleSource = sampling.source; return; } sampleSource = sampling.source; });
    let presetChoice = $state(-1);
    $effect(() => {
        const accepted = settings.customized ? -1 : settings.preset_id;
        if (saved.active) { presetChoice = accepted; return; }
        presetChoice = accepted;
    });
    function colorChange(event: Event) {
        const hex = (event.currentTarget as HTMLInputElement).value;
        if (/^#[0-9a-f]{6}$/i.test(hex)) bridge.paintCommand({ SetBrushColor: { color: hexToColor(hex) } });
    }
</script>
<section aria-labelledby="paint-heading">
    <header><span class="eyebrow">{direct ? 'UV SURFACE' : 'CANVAS → UV SURFACE'}</span><h2 id="paint-heading">{direct ? 'DirectUV paint' : 'Projection paint'}</h2></header>
    <div class="tool-choice" aria-label="Painting mode">
        <button type="button" aria-pressed={!direct} disabled={target.active} onclick={() => bridge.paintCommand({ SetTarget: { target: 'Canvas' } })}>Canvas projection</button>
        <button type="button" aria-pressed={direct} disabled={target.active || target.uv_layers?.projection_preview || !target.direct_available} onclick={() => bridge.paintCommand({ SetTarget: { target: 'DirectUv' } })}>DirectUV surface</button>
    </div>
    {#if target.notice}<p class="notice" role="status">{target.notice}</p>{/if}
    <div class="tool-choice" aria-label="Paint tool">
        <button type="button" disabled={brushLocked} aria-pressed={settings.blend_mode === 'Normal'} onclick={() => bridge.paintCommand({ SetBlendMode: { mode: 'Normal' } })}>Brush</button>
        <button type="button" disabled={brushLocked} aria-pressed={settings.blend_mode === 'Erase'} onclick={() => bridge.paintCommand({ SetBlendMode: { mode: 'Erase' } })}>Eraser</button>
    </div>
    {#if !direct}
    <button class="wide" type="button" aria-pressed={sampling.enabled} disabled={sampling.active} onclick={() => bridge.paintCommand({ SetColorSampling: { enabled: !sampling.enabled } })}>{sampling.enabled ? 'Cancel color sampling' : 'Sample canvas color'}</button>
    <label class="select-label" for="paint-sample-source">Sample source</label>
    <select id="paint-sample-source" disabled={sampling.active} bind:value={sampleSource} onchange={(e) => bridge.paintCommand({ SetColorSampleSource: { source: e.currentTarget.value as ColorSampleSource } })}>
        <option value="VisibleLayers">Visible layers</option><option value="ActiveLayer">Active layer</option>
    </select>
    <p class="hint">{sampling.enabled ? 'Click a painted source canvas pixel. Escape cancels sampling.' : 'Sample straight RGB from source canvas layers, ignoring transparency. Brush opacity and Brush / Eraser stay selected. Transparent pixels leave your color unchanged.'}</p>
    {/if}
    <label class="select-label" for="paint-preset">Brush preset</label>
    <select id="paint-preset" disabled={saved.active} bind:value={presetChoice} onchange={(e) => bridge.paintCommand({ SelectBrushPreset: { preset_id: Number(e.currentTarget.value) } })}>
        {#if settings.customized}<option value={-1} disabled>Custom round brush</option>{/if}
        {#each presets as preset}<option value={preset.id}>{preset.name}</option>{/each}
    </select>
    <p class="hint">Presets reset the tip settings. Your color and tool stay selected.</p>
    <SavedBrushPresets mode="Paint" catalog={saved} />
    <div class="tip-row">
        <div class="tip-preview" aria-label="Round tip preview" style={`background: radial-gradient(circle, ${toHex(settings.color)} ${settings.hardness * 45}%, transparent 50%); opacity: ${settings.opacity};`}></div>
        <div><strong>Round tip</strong><p class="hint">Hardness controls edge falloff.</p></div>
    </div>
    <BrushControl disabled={brushLocked} id="paint-radius" label="Radius" value={settings.size / 2} min={0.5} max={256} step={0.5} unit={direct ? 'atlas px' : 'canvas px'} onchange={(radius) => bridge.paintCommand({ SetBrushSize: { size: radius * 2 } })} />
    <BrushControl disabled={brushLocked} id="paint-opacity" label="Opacity" value={Math.round(settings.opacity * 100)} min={0} max={100} unit="%" onchange={(opacity) => bridge.paintCommand({ SetBrushOpacity: { opacity: opacity / 100 } })} />
    <BrushControl disabled={brushLocked} id="paint-hardness" label="Hardness / falloff" value={Math.round(settings.hardness * 100)} min={0} max={100} unit="%" onchange={(hardness) => bridge.paintCommand({ SetBrushHardness: { hardness: hardness / 100 } })} />
    <BrushControl disabled={brushLocked} id="paint-spacing" label="Dab spacing" value={Math.round(settings.spacing * 100)} min={1} max={100} unit="% of diameter" onchange={(spacing) => bridge.paintCommand({ SetBrushSpacing: { spacing: spacing / 100 } })} />
    <label class="color-row" for="paint-color">Color <input id="paint-color" type="color" value={toHex(settings.color)} disabled={brushLocked || settings.blend_mode === 'Erase'} oninput={colorChange} /><input class="hex-color" aria-label="Hex color" type="text" maxlength="7" pattern="#[0-9A-Fa-f]{6}" value={toHex(settings.color)} disabled={brushLocked || settings.blend_mode === 'Erase'} onchange={colorChange} /></label>
    <div class="divider"></div>
    <button class="wide" type="button" disabled={!canUndo} title={direct ? (target.uv_layers?.enabled ? 'Undo the last UV edit (Ctrl+Z)' : 'Undo the last DirectUV stroke (Ctrl+Z)') : 'Undo the active canvas stroke (Ctrl+Z)'} onclick={() => bridge.paintCommand('Undo')}>{direct ? (target.uv_layers?.enabled ? 'Undo UV edit' : 'Undo surface stroke') : 'Undo canvas stroke'}</button>
    <button class="wide" type="button" disabled={!canRedo} title={direct ? (target.uv_layers?.enabled ? 'Redo the last UV edit (Ctrl+Shift+Z)' : 'Redo the last DirectUV stroke (Ctrl+Shift+Z)') : 'Redo the active canvas stroke (Ctrl+Shift+Z)'} onclick={() => bridge.paintCommand('Redo')}>{direct ? (target.uv_layers?.enabled ? 'Redo UV edit' : 'Redo surface stroke') : 'Redo canvas stroke'}</button>
    <button class="wide" type="button" disabled={!target.active} onclick={() => bridge.paintCommand('CancelStroke')}>Cancel current stroke</button>
    {#if direct}
        <p class="hint">Paint the visible UV-mapped surface. Strokes blend into its editable texture; Undo and Redo target {target.target_name ?? 'the last painted receiver'}. Escape cancels the current stroke. The source canvas is hidden and live projection is paused.</p>
        <p class="hint" role="status">History: {(target.retained_bytes / 1048576).toFixed(1)} / {(target.limit_bytes / 1048576).toFixed(0)} MiB retained; {(target.pending_bytes / 1048576).toFixed(1)} MiB pending.{target.evicted_strokes > 0 ? ` ${target.evicted_strokes} older edits expired.` : ''}</p>
    {:else}
    <label class="check-row"><input type="checkbox" checked={liveProjection} disabled={target.active || (target.uv_layers?.enabled && target.uv_layers.conflicted)} onchange={(e) => {const enabled=e.currentTarget.checked;if(target.uv_layers?.enabled)e.currentTarget.checked=liveProjection;bridge.paintCommand({ SetLiveProjection: { enabled } });}} /> {target.uv_layers?.enabled ? 'Live UV preview' : 'Live projection'}</label>
    <label class="check-row"><input type="checkbox" checked={sourceVisible} onchange={(e) => bridge.paintCommand({ SetSourceVisible: { visible: e.currentTarget.checked } })} /> Show source canvas</label>
    <button class="wide primary" type="button" disabled={target.active} onclick={() => bridge.paintCommand('ProjectToScene')}>{target.uv_layers?.enabled ? 'Apply canvas to active UV layer' : 'Apply canvas to UV surfaces'}</button>
    {#if target.uv_layers?.enabled}<button class="wide" type="button" disabled={!target.uv_layers.projection_preview} onclick={()=>bridge.paintCommand('CancelUvProjection')}>Cancel UV preview</button><p class="hint">Live preview follows Canvas strokes and source Undo/Redo on the pinned UV layer. Apply commits once and pauses live. Cancel preview retains Canvas edits and UV history. Apply or Cancel before DirectUV, layer edits or Save.</p>{:else}<p class="hint">Paint on the source canvas, then apply to visible UV-mapped meshes. Live projection also follows stroke Undo and Redo. Hide the source to inspect or paint the projected surface; the canvas remains the brush target.</p>{/if}
    {/if}
    {#if target.uv_layers}<UvLayerPanel layerState={target.uv_layers} history={target} />{/if}
    <div class="divider"></div>
    <p class="hint">{direct ? "Shift + middle-drag: pan. Middle-drag: orbit. Scroll: zoom." : "Shift + middle-drag: pan. Scroll: zoom. Tab: leave / return to canvas view. Middle-drag orbits outside canvas view."}</p>
</section>
