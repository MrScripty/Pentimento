<script lang="ts">
    import { bridge } from '$lib/bridge';
    import type { UvLayerState, UvLayerCommand, UvLayerBlendMode, UvLayerPaintTarget, PaintTargetState } from '$lib/types';
    let { layerState, history }: {layerState:UvLayerState;history:PaintTargetState} = $props();
    let name=$state('');
    let locked=$derived(layerState.active || layerState.conflicted);
    let active=$derived(layerState.layers.find(l=>l.is_active));
    const percent=(opacity:number)=>Number((opacity*100).toFixed(3));
    const send=(command:UvLayerCommand)=>bridge.paintCommand({UvLayers:{command}});
    const blendModes:UvLayerBlendMode[]=['Normal','Multiply','Screen','Overlay'];
</script>
<section aria-labelledby="uv-layer-heading">
    <div class="divider"></div><h3 id="uv-layer-heading">UV texture layers</h3>
    <label class="select-label" for="uv-receiver">Paint receiver</label>
    <select id="uv-receiver" disabled={layerState.active} value={layerState.receiver ?? ''} onchange={e=>{const mesh_id=Number(e.currentTarget.value);e.currentTarget.value=String(layerState.receiver ?? '');send({SelectReceiver:{mesh_id}});}}>
        <option value="" disabled>Select a UV receiver</option>
        {#each layerState.receivers as receiver}<option value={receiver.mesh_id}>{receiver.name}</option>{/each}
    </select>
    {#if !layerState.enabled}
        <button class="wide" disabled={locked || layerState.receiver===null} onclick={()=>send('Enable')}>Enable UV texture layers</button>
        <p class="hint">Keep exact existing pixels as editable layers. This enables linear Normal compositing; legacy appearance may change. Existing canvas projections become independent snapshots.</p>
    {:else}
        <p class="hint">DirectUV and Canvas Apply paint the selected layer’s Color or Mask target. Modes blend with lower visible UV layers in linear color; the UV stack then overlays the original material. A lone layer behaves like Normal. Hidden or locked layers refuse painting.</p>
        <label class="select-label" for="uv-new-name">New UV layer name</label>
        <input id="uv-new-name" type="text" bind:value={name} maxlength="256" disabled={locked} />
        <button class="wide" disabled={locked} onclick={()=>send({Create:{name}})}>Create UV layer</button>
        <div class="uv-layers" role="list" aria-label="UV layer stack, top first">
            {#each layerState.layers as layer,index (layer.id)}
                <div role="listitem" class="uv-layer" class:chosen={layer.is_active}>
                    <button class="wide" disabled={locked} aria-pressed={layer.is_active} onclick={()=>send({Select:{layer_id:layer.id}})}>{layer.is_active ? 'Paint target: ' : ''}{layer.name}</button>
                    <div class="uv-toggles">
                        <label><input type="checkbox" aria-label={`Show UV layer ${layer.name}`} checked={layer.visible} disabled={locked} onchange={e=>{const visible=e.currentTarget.checked;e.currentTarget.checked=layer.visible;send({Visible:{layer_id:layer.id,visible}});}} /> Show</label>
                        <label><input type="checkbox" aria-label={`Lock UV layer ${layer.name}`} checked={layer.locked} disabled={locked} onchange={e=>{const locked=e.currentTarget.checked;e.currentTarget.checked=layer.locked;send({Lock:{layer_id:layer.id,locked}});}} /> Lock paint</label>
                    </div>
                    {#if layer.is_active}
                        <div class="uv-actions">
                            {#if layer.has_mask}
                                <button disabled={locked} onclick={()=>send({RemoveMask:{layer_id:layer.id}})}>Remove layer mask</button>
                            {:else}
                                <button disabled={locked} onclick={()=>send({AddMask:{layer_id:layer.id}})}>Add layer mask</button>
                            {/if}
                        </div>
                        {#if layer.has_mask}
                            <label class="check-row"><input type="checkbox" aria-label="Enable layer mask" checked={layer.mask_enabled} disabled={locked} onchange={e=>{const enabled=e.currentTarget.checked;e.currentTarget.checked=layer.mask_enabled;send({MaskEnabled:{layer_id:layer.id,enabled}});}} /> Enable layer mask</label>
                        {/if}
                        <label class="select-label" for={`uv-target-${layer.id}`}>Paint color or mask</label>
                        <select id={`uv-target-${layer.id}`} value={layer.paint_target} disabled={locked} onchange={e=>{const target=e.currentTarget.value as UvLayerPaintTarget;e.currentTarget.value=layer.paint_target;send({PaintTarget:{layer_id:layer.id,target}});}}>
                            <option value="Color">Color</option><option value="Mask" disabled={!layer.has_mask || !layer.mask_enabled}>Mask</option>
                        </select>
                        <p class="hint">Masks begin white. White reveals layer color; black hides it. Disabling a mask keeps its pixels and selects Color. Mask edits have their own stroke history within UV Undo.</p>
                        <label class="select-label" for={`uv-blend-${layer.id}`}>UV layer blend mode</label>
                        <select id={`uv-blend-${layer.id}`} value={layer.blend_mode} disabled={locked} onchange={e=>{const mode=e.currentTarget.value as UvLayerBlendMode;e.currentTarget.value=layer.blend_mode;send({BlendMode:{layer_id:layer.id,mode}});}}>
                            {#each blendModes as mode}<option value={mode}>{mode}</option>{/each}
                        </select>
                        <label class="select-label" for={`uv-name-${layer.id}`}>UV layer name</label>
                        <input id={`uv-name-${layer.id}`} type="text" value={layer.name} maxlength="256" disabled={locked} onchange={e=>{const name=e.currentTarget.value;e.currentTarget.value=layer.name;send({Rename:{layer_id:layer.id,name}});}} />
                        <label class="select-label" for={`uv-opacity-${layer.id}`}>UV layer opacity (%)</label>
                        <input id={`uv-opacity-${layer.id}`} type="number" value={percent(layer.opacity)} min="0" max="100" step="0.1" disabled={locked} onchange={e=>{const opacity=e.currentTarget.value==='' ? NaN : Number(e.currentTarget.value)/100;e.currentTarget.value=String(percent(layer.opacity));if(Number.isFinite(opacity))send({Opacity:{layer_id:layer.id,opacity}});}} />
                        <div class="uv-actions">
                            <button disabled={locked || index===0} onclick={()=>send({Reorder:{layer_id:layer.id,new_index:layerState.layers.length-index}})}>Raise UV layer</button>
                            <button disabled={locked || index===layerState.layers.length-1} onclick={()=>send({Reorder:{layer_id:layer.id,new_index:layerState.layers.length-index-2}})}>Lower UV layer</button>
                        </div>
                    {/if}
                </div>
            {/each}
        </div>
        <div class="uv-actions">
            <button disabled={locked || !active} onclick={()=>active && send({Duplicate:{layer_id:active.id}})}>Duplicate UV layer</button>
            <button disabled={locked || !active || layerState.layers.length===1} onclick={()=>active && send({Delete:{layer_id:active.id}})}>Delete UV layer</button>
        </div>
        <button class="wide" disabled={locked || !layerState.can_undo} onclick={()=>send('Undo')}>Undo UV layer edit</button>
        <button class="wide" disabled={locked || !layerState.can_redo} onclick={()=>send('Redo')}>Redo UV layer edit</button>
        <p class="hint" role="status">History payload: {(history.retained_bytes/1048576).toFixed(1)} / {(history.limit_bytes/1048576).toFixed(0)} MiB retained; {(history.pending_bytes/1048576).toFixed(1)} MiB pending.{history.evicted_strokes>0 ? ` ${history.evicted_strokes} older edits expired.` : ''}</p>
        <p class="hint">UV Undo includes strokes, Apply and layer changes. Layer and paint-target selection preserve Redo. Canvas stroke Undo edits the source canvas separately. Delete is recoverable while its bounded history remains available.</p>
    {/if}
    {#if layerState.projection_preview}<p class="notice" role="status">Live preview owns this receiver, layer and paint target. Apply or Cancel preview before changing the stack.</p>{/if}
    {#if layerState.conflicted}<p role="status" class="notice">UV ownership changed. Reopen the owned project before editing this receiver.</p>{/if}
</section>
<style>
    h3 {font-size:14px;margin:10px 0}.uv-layer {padding:8px;border:1px solid #46474c;border-radius:6px;margin:8px 0}.chosen {border-color:#92b2fa}
    .uv-toggles,.uv-actions {display:flex;gap:8px;margin:8px 0;font-size:11px}.uv-actions>* {flex:1}
    input[type='text'],input[type='number'] {width:100%;box-sizing:border-box;background:#202329;border:1px solid #52545c;border-radius:4px;color:#eee;padding:6px}
</style>
