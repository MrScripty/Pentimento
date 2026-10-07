<script lang="ts">
    import { onMount } from 'svelte';
    import { bridge } from '$lib/bridge';
    import type { EditMode, PaintBrushSettings, PaintBrushPresetInfo, SculptBrushSettings } from '$lib/types';
    import PaintBrushPanel from './PaintBrushPanel.svelte';
    import SculptBrushPanel from './SculptBrushPanel.svelte';
    let { mode }: { mode: EditMode } = $props();
    let paint = $state<PaintBrushSettings | null>(null);
    let sculpt = $state<SculptBrushSettings | null>(null);
    let presets = $state<PaintBrushPresetInfo[]>([]);
    let sculptReceived = $state(false);
    let canUndo = $state(false);
    let liveProjection = $state(false);
    onMount(() => {
        const unsubscribe = bridge.subscribe(message => {
            if (message.type === 'PaintBrushStateChanged') {
                paint = message.data.settings; presets = message.data.presets; canUndo = message.data.can_undo;
            } else if (message.type === 'SculptBrushStateChanged') {
                sculpt = message.data.settings; sculptReceived = true;
            } else if (message.type === 'ProjectionModeChanged') {
                liveProjection = message.data.live_projection;
            }
        });
        bridge.requestBrushState();
        return unsubscribe;
    });
</script>
{#if mode === 'Paint' || mode === 'Sculpt'}
    <aside class="brush-panel panel interactive" data-ui-region="brush-panel" aria-label={mode === 'Paint' ? 'Projection paint controls' : 'Sculpt controls'}>
        {#if mode === 'Paint' && paint}<PaintBrushPanel settings={paint} {presets} {canUndo} {liveProjection} />
        {:else if mode === 'Sculpt' && sculpt}<SculptBrushPanel settings={sculpt} />
        {:else if mode === 'Sculpt' && sculptReceived}<p class="notice">Sculpting is not available in this renderer build.</p>
        {:else}<p class="notice">Waiting for brush settings from the renderer.</p>{/if}
    </aside>
{/if}
<style>
    .brush-panel { position: fixed; top: 60px; right: 12px; bottom: 12px; width: min(300px, calc(100vw - 24px)); border-radius: 10px; padding: 18px; overflow-y: auto; z-index: 150; color: #eee; font-family: system-ui, sans-serif; }
    .brush-panel :global(h2) { margin: 4px 0 18px; font-size: 20px; font-weight: 600; }
    .brush-panel :global(.eyebrow) { font-size: 9px; letter-spacing: .12em; color: #a4baff; }
    .brush-panel :global(button) { padding: 8px 10px; border: 1px solid #52545c; border-radius: 6px; background: #282b33; color: #eee; cursor: pointer; font-size: 12px; }
    .brush-panel :global(button:hover) { background: #394154; }
    .brush-panel :global(button[aria-pressed='true']), .brush-panel :global(.primary) { background: #344b79; border-color: #92b2fa; }
    .brush-panel :global(button:disabled) { opacity: .45; cursor: default; }
    .brush-panel :global(button:focus-visible), .brush-panel :global(select:focus-visible), .brush-panel :global(input:focus-visible) { outline: 2px solid #9abaff; outline-offset: 2px; }
    .brush-panel :global(.tool-choice), .brush-panel :global(.sculpt-tools) { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-bottom: 14px; }
    .brush-panel :global(.hint) { color: #aaaeb9; font-size: 11px; line-height: 1.5; margin: 8px 0; }
    .brush-panel :global(.notice) { color: #dfc59d; font-size: 12px; line-height: 1.5; }
    .brush-panel :global(.select-label) { display: block; font-size: 12px; margin: 12px 0 6px; }
    .brush-panel :global(select) { width: 100%; padding: 7px; border: 1px solid #52545c; border-radius: 5px; background: #202329; color: #eee; }
    .brush-panel :global(.tip-row) { display: flex; align-items: center; gap: 10px; margin-top: 14px; font-size: 12px; }
    .brush-panel :global(.tip-preview) { width: 48px; height: 48px; background-color: #eee; }
    .brush-panel :global(.color-row) { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 12px; margin-top: 16px; }
    .brush-panel :global(.hex-color) { color: #ddd; font-family: monospace; width: 78px; border: 1px solid #52545c; border-radius: 4px; background: #202329; padding: 5px; }
    .brush-panel :global(input[type='color']) { width: 50px; height: 30px; border: none; background: transparent; }
    .brush-panel :global(.check-row) { display: flex; align-items: center; gap: 8px; font-size: 12px; padding: 12px 0; }
    .brush-panel :global(.wide) { width: 100%; }
    .brush-panel :global(.divider) { height: 1px; background: #46474c; margin: 16px 0; }
</style>
