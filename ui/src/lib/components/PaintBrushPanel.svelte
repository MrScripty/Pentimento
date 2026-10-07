<script lang="ts">
    import { bridge } from '$lib/bridge';
    import { colorToHex as toHex, hexToColor } from '$lib/brush-values';
    import type { PaintBrushSettings, PaintBrushPresetInfo } from '$lib/types';
    import BrushControl from './BrushControl.svelte';
    interface Props { settings: PaintBrushSettings; presets: PaintBrushPresetInfo[]; canUndo: boolean; liveProjection: boolean }
    let { settings, presets, canUndo, liveProjection }: Props = $props();
    function colorChange(event: Event) {
        const hex = (event.currentTarget as HTMLInputElement).value;
        if (/^#[0-9a-f]{6}$/i.test(hex)) bridge.paintCommand({ SetBrushColor: { color: hexToColor(hex) } });
    }
</script>
<section aria-labelledby="paint-heading">
    <header><span class="eyebrow">CANVAS → UV SURFACE</span><h2 id="paint-heading">Projection paint</h2></header>
    <div class="tool-choice" aria-label="Paint tool">
        <button type="button" aria-pressed={settings.blend_mode === 'Normal'} onclick={() => bridge.paintCommand({ SetBlendMode: { mode: 'Normal' } })}>Brush</button>
        <button type="button" aria-pressed={settings.blend_mode === 'Erase'} onclick={() => bridge.paintCommand({ SetBlendMode: { mode: 'Erase' } })}>Eraser</button>
    </div>
    <label class="select-label" for="paint-preset">Brush preset</label>
    <select id="paint-preset" value={settings.customized ? -1 : settings.preset_id} onchange={(e) => bridge.paintCommand({ SelectBrushPreset: { preset_id: Number(e.currentTarget.value) } })}>
        {#if settings.customized}<option value={-1} disabled>Custom round brush</option>{/if}
        {#each presets as preset}<option value={preset.id}>{preset.name}</option>{/each}
    </select>
    <p class="hint">Presets reset the tip settings. Your color and tool stay selected.</p>
    <div class="tip-row">
        <div class="tip-preview" aria-label="Round tip preview" style={`background: radial-gradient(circle, ${toHex(settings.color)} ${settings.hardness * 45}%, transparent 50%); opacity: ${settings.opacity};`}></div>
        <div><strong>Round tip</strong><p class="hint">Hardness controls edge falloff.</p></div>
    </div>
    <BrushControl id="paint-radius" label="Radius" value={settings.size / 2} min={0.5} max={256} step={0.5} unit="canvas px" onchange={(radius) => bridge.paintCommand({ SetBrushSize: { size: radius * 2 } })} />
    <BrushControl id="paint-opacity" label="Opacity" value={Math.round(settings.opacity * 100)} min={0} max={100} unit="%" onchange={(opacity) => bridge.paintCommand({ SetBrushOpacity: { opacity: opacity / 100 } })} />
    <BrushControl id="paint-hardness" label="Hardness / falloff" value={Math.round(settings.hardness * 100)} min={0} max={100} unit="%" onchange={(hardness) => bridge.paintCommand({ SetBrushHardness: { hardness: hardness / 100 } })} />
    <BrushControl id="paint-spacing" label="Dab spacing" value={Math.round(settings.spacing * 100)} min={1} max={100} unit="% of diameter" onchange={(spacing) => bridge.paintCommand({ SetBrushSpacing: { spacing: spacing / 100 } })} />
    <label class="color-row" for="paint-color">Color <input id="paint-color" type="color" value={toHex(settings.color)} disabled={settings.blend_mode === 'Erase'} oninput={colorChange} /><input class="hex-color" aria-label="Hex color" type="text" maxlength="7" pattern="#[0-9A-Fa-f]{6}" value={toHex(settings.color)} disabled={settings.blend_mode === 'Erase'} onchange={colorChange} /></label>
    <div class="divider"></div>
    <button class="wide" type="button" disabled={!canUndo} title="Undo the active canvas stroke (Ctrl+Z)" onclick={() => bridge.paintCommand('Undo')}>Undo canvas stroke</button>
    <label class="check-row"><input type="checkbox" checked={liveProjection} onchange={(e) => bridge.paintCommand({ SetLiveProjection: { enabled: e.currentTarget.checked } })} /> Live projection</label>
    <button class="wide primary" type="button" onclick={() => bridge.paintCommand('ProjectToScene')}>Apply canvas to UV surfaces</button>
    <p class="hint">Paint on the source canvas, then apply to visible UV-mapped meshes. Live projection updates those surfaces while you paint.</p>
    <div class="divider"></div>
    <p class="hint">Shift + middle-drag: pan. Scroll: zoom. Tab: leave / return to canvas view. Middle-drag orbits outside canvas view.</p>
</section>
