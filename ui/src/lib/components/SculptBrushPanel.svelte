<script lang="ts">
    import { bridge } from '$lib/bridge';
    import type { SculptBrushSettings, SculptTool, SculptFalloff, SculptHistoryState } from '$lib/types';
    import BrushControl from './BrushControl.svelte';
    let { settings, history }: { settings: SculptBrushSettings; history: SculptHistoryState } = $props();
    const tools: { name: SculptTool; description: string }[] = [
        { name: 'Push', description: 'Move along the hit surface normal' },
        { name: 'Pull', description: 'Draw vertices toward the brush center' },
        { name: 'Grab', description: 'Drag the surface along your stroke' },
        { name: 'Smooth', description: 'Average neighboring vertex positions' },
        { name: 'Flatten', description: 'Level the surface toward a plane' },
        { name: 'Inflate', description: 'Expand along vertex normals' },
        { name: 'Pinch', description: 'Gather the surface inward' },
        { name: 'Crease', description: 'Carve a crease along the stroke' },
    ];
    const falloffs: SculptFalloff[] = ['Linear', 'Smooth', 'Sharp', 'Constant', 'Sphere'];
</script>
<section aria-labelledby="sculpt-heading">
    <header><span class="eyebrow">MESH SCULPTING</span><h2 id="sculpt-heading">Sculpt brushes</h2></header>
    <div class="sculpt-tools" aria-label="Sculpt tool">
        {#each tools as tool}<button type="button" aria-pressed={settings.tool === tool.name} title={tool.description} onclick={() => bridge.sculptCommand({ SetTool: { tool: tool.name } })}>{tool.name}</button>{/each}
    </div>
    <p class="hint">{tools.find(tool => tool.name === settings.tool)?.description}. Custom settings stay selected when switching tools.</p>
    <BrushControl id="sculpt-radius" label="Radius" value={Number(settings.radius.toFixed(2))} min={0.01} max={10} step={0.01} unit="mesh units" onchange={(radius) => bridge.sculptCommand({ SetRadius: { radius } })} />
    <p class="hint">Object scale affects the brush footprint.</p>
    <BrushControl id="sculpt-strength" label="Strength" value={Math.round(settings.strength * 100)} min={0} max={100} unit="%" onchange={(strength) => bridge.sculptCommand({ SetStrength: { strength: strength / 100 } })} />
    <BrushControl id="sculpt-hardness" label="Hardness" value={Math.round(settings.hardness * 100)} min={0} max={100} unit="%" onchange={(hardness) => bridge.sculptCommand({ SetHardness: { hardness: hardness / 100 } })} />
    <label class="select-label" for="sculpt-falloff">Falloff curve</label>
    <select id="sculpt-falloff" value={settings.falloff} onchange={(e) => bridge.sculptCommand({ SetFalloff: { falloff: e.currentTarget.value as SculptFalloff } })}>{#each falloffs as falloff}<option>{falloff}</option>{/each}</select>
    <p class="hint">Hardness defines the full-strength center. The curve controls the edge.</p>
    <div class="divider"></div>
    <div class="sculpt-tools" aria-label="Sculpt history">
        <button type="button" disabled={history.active || history.undo_strokes === 0} title="Undo sculpt stroke (Ctrl+Z)" onclick={() => bridge.sculptCommand('Undo')}>Undo sculpt stroke</button>
        <button type="button" disabled={history.active || history.redo_strokes === 0} title="Redo sculpt stroke (Ctrl+Shift+Z)" onclick={() => bridge.sculptCommand('Redo')}>Redo sculpt stroke</button>
    </div>
    <p class="hint">{history.undo_strokes} undo / {history.redo_strokes} redo. History lasts for this sculpt session. Escape rolls back an active stroke.</p>
    {#if history.notice}<p class="notice" role="status">{history.notice}</p>{/if}
    <p class="hint">F: adjust radius. Shift + F: adjust strength. Click or Enter to confirm; Escape to cancel the adjustment.</p>
    <p class="hint">Middle-drag: orbit. Shift + middle-drag: pan. Scroll: zoom. Ctrl + Tab: leave sculpt mode.</p>
</section>
