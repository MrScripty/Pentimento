<script lang="ts">
    import { bridge } from '$lib/bridge';
    import type { SavedBrushPresetsState } from '$lib/types';
    let { mode, catalog }: { mode: 'Paint' | 'Sculpt'; catalog: SavedBrushPresetsState } = $props();
    let name = $state('');
    let chosen = $state('');
    const label = $derived(mode === 'Paint' ? 'paint' : 'sculpt');
    const entries = $derived(mode === 'Paint' ? catalog.paint : catalog.sculpt);
    const selected = $derived(mode === 'Paint' ? catalog.selected_paint : catalog.selected_sculpt);
    $effect(() => {
        // Native ownership cancels a pending choice; current selection is always
        // displayed from the accepted snapshot rather than optimistic commands.
        if (catalog.active) { chosen = selected === null ? '' : String(selected); return; }
        chosen = selected === null ? '' : String(selected);
    });
    function save() {
        if (catalog.active || !catalog.available || !name.trim()) return;
        const command = { SaveBrushPreset: { name: name.trim() } };
        if (mode === 'Paint') bridge.paintCommand(command); else bridge.sculptCommand(command);
    }
    function restore() {
        if (catalog.active || !catalog.available || !entries.some(entry => String(entry.id) === chosen)) return;
        const command = { SelectSavedBrushPreset: { preset_id: Number(chosen) } };
        if (mode === 'Paint') bridge.paintCommand(command); else bridge.sculptCommand(command);
    }
</script>
<div class="saved-presets">
    <label class="select-label" for={`saved-${label}-preset`}>Saved {label} brushes</label>
    <select id={`saved-${label}-preset`} bind:value={chosen} disabled={catalog.active || !catalog.available || entries.length === 0}>
        <option value="">Choose a saved brush</option>
        {#each entries as entry}<option value={String(entry.id)}>{entry.name}</option>{/each}
    </select>
    <button class="wide" type="button" disabled={catalog.active || !catalog.available || !chosen} onclick={restore}>Use {label} brush</button>
    <label class="select-label" for={`saved-${label}-name`}>{mode} preset name</label>
    <input id={`saved-${label}-name`} type="text" maxlength="64" bind:value={name} disabled={catalog.active || !catalog.available} />
    <button class="wide" type="button" disabled={catalog.active || !catalog.available || !name.trim()} onclick={save}>Save {label} brush</button>
    <p class="hint">{mode === 'Paint' ? 'Saves tip settings, pressure sizes, color and brush/eraser.' : 'Saves tool, radius, strength, hardness, falloff and auto smoothing.'} Saved on this computer. Reusing a name replaces that brush. Up to 64 per mode.</p>
    {#if selected !== null}<p class="hint">Current saved brush: {entries.find(entry => entry.id === selected)?.name}</p>{/if}
    {#if catalog.notice}<p class="notice" role="status">{catalog.notice}</p>{/if}
</div>
<style>
    input { box-sizing: border-box; width: 100%; padding: 7px; color: #eee; background: #202329; border: 1px solid #52545c; border-radius: 5px; }
    button { margin-top: 8px; }
    input:disabled, select:disabled { opacity: .45; }
</style>
