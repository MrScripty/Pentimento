// Compiled Svelte server rendering checks controls without launching a browser
// or changing sandbox settings. Interactive desktop qualification is separate.
import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'vite';
let server, Panel, render;
const previousWindow=globalThis.window;
before(async()=>{
    const events=new EventTarget();
    globalThis.window={addEventListener:events.addEventListener.bind(events),removeEventListener:events.removeEventListener.bind(events)};
    server=await createServer({server:{middlewareMode:true,watch:null}});
    render=(await server.ssrLoadModule('svelte/server')).render;
    Panel=(await server.ssrLoadModule('/src/lib/components/PaintBrushPanel.svelte')).default;
});
after(async()=>{await server?.close();if(previousWindow===undefined)delete globalThis.window;else globalThis.window=previousWindow;});
function html({mask=true,enabled=true,channel='Mask',mode='DirectUv',active=false}={}) {
    const layer={id:0,name:'Detail',visible:true,opacity:0.5,locked:false,is_active:true,blend_mode:'Overlay',has_mask:mask,mask_enabled:enabled,paint_target:channel};
    return render(Panel,{props:{
        target:{mode,direct_available:true,target_name:'Receiver',active,notice:null,retained_bytes:0,pending_bytes:0,limit_bytes:67108864,evicted_strokes:0,uv_layers:{receivers:[{mesh_id:12,name:'Receiver',layered:true}],receiver:12,enabled:true,layers:[layer],can_undo:true,can_redo:false,active,projection_preview:false,conflicted:false,notice:null}},
        settings:{preset_id:0,customized:false,color:[0.25,0.25,0.25,1],size:8,opacity:1,hardness:0.5,spacing:0.25,blend_mode:'Normal'},
        presets:[{id:0,name:'Round'}],canUndo:true,canRedo:false,sourceVisible:true,liveProjection:false,
        saved:{paint:[],sculpt:[],selected_paint:null,selected_sculpt:null,active:false,available:true,notice:null},
        sampling:{enabled:false,active:false,source:'VisibleLayers'}
    }}).body;
}
test('mask controls render authoritative enable, channel, history and direct grayscale',()=>{
    const output=html();
    assert.match(output,/Remove layer mask/);assert.match(output,/aria-label="Enable layer mask"[^>]*checked/);
    assert.match(output,/<option value="Mask"[^>]*selected/);assert.match(output,/Mask grayscale value/);
    assert.doesNotMatch(output,/aria-label="Hex color"/);assert.match(output,/Undo UV layer edit/);assert.match(output,/Redo UV layer edit/);
});
test('absent or disabled masks expose Color and refuse Mask selection in controls',()=>{
    const absent=html({mask:false,enabled:false,channel:'Color'});
    assert.match(absent,/Add layer mask/);assert.doesNotMatch(absent,/aria-label="Enable layer mask"/);
    assert.match(absent,/<option value="Mask"[^>]*disabled/);assert.match(absent,/aria-label="Hex color"/);
    const disabled=html({enabled:false,channel:'Color'});
    assert.match(disabled,/Remove layer mask/);assert.match(disabled,/<option value="Mask"[^>]*disabled/);
    assert.doesNotMatch(disabled,/aria-label="Enable layer mask"[^>]*checked/);
    assert.match(html({active:true}),/<select id="uv-target-0"[^>]*disabled/);
});
test('Canvas mask target retains source color controls and explains Apply brightness',()=>{
    const output=html({mode:'Canvas'});
    assert.match(output,/aria-label="Hex color"/);assert.doesNotMatch(output,/Mask grayscale value/);
    assert.match(output,/linear brightness as mask coverage/);assert.match(output,/Apply canvas to active UV layer/);
});
