import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

function loadSamples() {
  const result = spawnSync(
    'cargo',
    ['run', '--quiet', '-p', 'pentimento-ipc', '--example', 'contract_samples'],
    {
      cwd: process.cwd(),
      encoding: 'utf8'
    }
  );

  if (result.status !== 0) {
    throw result.error ?? new Error(result.stderr || 'failed to generate contract samples');
  }

  return JSON.parse(result.stdout);
}

function assertTuple(value, length, label) {
  assert.ok(Array.isArray(value), `${label} must be an array`);
  assert.equal(value.length, length, `${label} must have ${length} entries`);
}

function assertLayerInfo(layer) {
  assert.equal(typeof layer.id, 'number');
  assert.equal(typeof layer.name, 'string');
  assert.equal(typeof layer.visible, 'boolean');
  assert.equal(typeof layer.opacity, 'number');
  assert.equal(typeof layer.is_active, 'boolean');
}

function assertBevyToUiMessage(message) {
  assert.equal(typeof message.type, 'string');

  switch (message.type) {
    case 'ProjectStateChanged':
      for(const field of ['available','active','blocked'])assert.equal(typeof message.data[field],'boolean');
      assert.ok(message.data.path===null||typeof message.data.path==='string');
      assert.ok(message.data.notice===null||typeof message.data.notice==='string');return;
    case 'ProjectOperationFinished':
      assert.match(message.data.operation,/^(Save|Open)$/);assert.equal(typeof message.data.success,'boolean');assert.equal(typeof message.data.message,'string');return;
    case 'Initialize':
      assert.ok(message.data);
      assert.ok(Array.isArray(message.data.scene_info.objects));
      assert.equal(typeof message.data.settings.render_scale, 'number');
      return;
    case 'ShowAddObjectMenu':
      assert.equal(typeof message.data.show, 'boolean');
      if (message.data.position !== null) {
        assertTuple(message.data.position, 2, 'ShowAddObjectMenu.position');
      }
      return;
    case 'AmbientOcclusionChanged':
      assert.equal(typeof message.data.settings.enabled, 'boolean');
      assert.equal(typeof message.data.settings.quality_level, 'number');
      return;
    case 'EditModeChanged':
      assert.match(message.data.mode, /^(None|Paint|MeshEdit|Sculpt)$/);
      return;
    case 'MeshEditModeChanged':
      assert.equal(typeof message.data.active, 'boolean');
      assert.match(message.data.selection_mode, /^(Vertex|Edge|Face)$/);
      assert.match(message.data.tool, /^(Select|Extrude|LoopCut|Knife|Merge|Inset)$/);
      return;
    case 'LayerStateChanged':
      assert.ok(Array.isArray(message.data.layers));
      message.data.layers.forEach(assertLayerInfo);
      return;
    case 'PaintColorSamplingChanged':
      assert.equal(typeof message.data.enabled, 'boolean');
      assert.equal(typeof message.data.active, 'boolean');
      assert.match(message.data.source, /^(VisibleLayers|ActiveLayer)$/);
      return;
    case 'PaintBrushStateChanged':
      assertTuple(message.data.settings.color, 4, 'paint color');
      for (const field of ['size', 'opacity', 'hardness', 'spacing', 'preset_id']) assert.equal(typeof message.data.settings[field], 'number');
      assert.match(message.data.settings.blend_mode, /^(Normal|Erase)$/);
      assert.equal(typeof message.data.settings.customized, 'boolean');
      assert.ok(Array.isArray(message.data.presets));
      assert.equal(typeof message.data.can_undo, 'boolean');
      assert.equal(typeof message.data.can_redo, 'boolean');
      assert.equal(typeof message.data.source_visible, 'boolean');
      const target = message.data.target;
      assert.match(target.mode, /^(Canvas|DirectUv)$/);
      for (const field of ['direct_available', 'active']) assert.equal(typeof target[field], 'boolean');
      for (const field of ['retained_bytes', 'pending_bytes', 'limit_bytes', 'evicted_strokes']) assert.equal(typeof target[field], 'number');
      for (const field of ['target_name', 'notice']) assert.ok(target[field] === null || typeof target[field] === 'string');
      return;
    case 'SavedBrushPresetsChanged':
      for (const mode of ['paint', 'sculpt']) {
        assert.ok(Array.isArray(message.data[mode]));
        for (const preset of message.data[mode]) { assert.equal(typeof preset.id, 'number'); assert.equal(typeof preset.name, 'string'); }
        assert.ok(message.data[`selected_${mode}`] === null || typeof message.data[`selected_${mode}`] === 'number');
      }
      assert.equal(typeof message.data.active, 'boolean');
      assert.equal(typeof message.data.available, 'boolean');
      assert.ok(message.data.notice === null || typeof message.data.notice === 'string');
      return;
    case 'SculptBrushStateChanged':
      if (message.data.settings !== null) {
        assert.match(message.data.settings.tool, /^(Push|Pull|Grab|Smooth|Flatten|Inflate|Pinch|Crease)$/);
        assert.match(message.data.settings.falloff, /^(Linear|Smooth|Sharp|Constant|Sphere)$/);
        for (const field of ['radius', 'strength', 'hardness', 'autosmooth']) assert.equal(typeof message.data.settings[field], 'number');
      }
      return;
    case 'SculptHistoryChanged':
      for (const field of ['undo_strokes', 'redo_strokes']) assert.equal(typeof message.data[field], 'number');
      assert.equal(typeof message.data.active, 'boolean');
      assert.ok(message.data.notice === null || typeof message.data.notice === 'string');
      return;
    case 'ProjectionModeChanged':
      assert.equal(typeof message.data.live_projection, 'boolean');
      return;
    case 'CloseMenus':
      assert.equal(message.data, undefined);
      return;
    default:
      throw new Error(`Unhandled BevyToUi sample type: ${message.type}`);
  }
}

function assertUiToBevyMessage(message) {
  assert.equal(typeof message.type, 'string');

  switch (message.type) {
    case 'ProjectCommand':
      if(message.data==='GetState')return;
      assert.equal(typeof (message.data.Save??message.data.Open)?.path,'string');return;
    case 'AddObject':
      assert.equal(typeof message.data.primitive_type, 'string');
      if (message.data.position !== null) {
        assertTuple(message.data.position, 3, 'AddObject.position');
      }
      return;
    case 'UpdateLighting':
      assert.equal(typeof message.data.time_of_day, 'number');
      assert.equal(typeof message.data.moon_phase, 'number');
      assert.equal(typeof message.data.azimuth_angle, 'number');
      assert.equal(typeof message.data.pollution, 'number');
      assertTuple(message.data.sun_direction, 3, 'UpdateLighting.sun_direction');
      return;
    case 'SetDepthView':
      assert.equal(typeof message.data.enabled, 'boolean');
      return;
    case 'AddPaintCanvas':
      assert.ok(message.data.width === null || typeof message.data.width === 'number');
      assert.ok(message.data.height === null || typeof message.data.height === 'number');
      return;
    case 'PaintCommand':
      if (typeof message.data === 'string') assert.ok(['Undo', 'Redo', 'CancelStroke', 'CancelUvProjection', 'ProjectToScene'].includes(message.data));
      else {
        assert.equal(typeof message.data, 'object');
        if (message.data.SetTarget) assert.match(message.data.SetTarget.target, /^(Canvas|DirectUv)$/);
      }
      return;
    case 'SculptCommand':
      if (typeof message.data === 'string') { assert.ok(['Undo', 'Redo'].includes(message.data)); return; }
      assert.equal(typeof message.data, 'object');
      assert.ok(['SetTool', 'SetRadius', 'SetStrength', 'SetHardness', 'SetFalloff', 'SetAutoSmooth', 'SaveBrushPreset', 'SelectSavedBrushPreset'].includes(Object.keys(message.data)[0]));
      return;
    case 'RequestBrushState':
      assert.equal(message.data, undefined);
      return;
    case 'SetUiInputCapture':
      assert.equal(typeof message.data.keyboard, 'boolean');
      return;
    case 'GizmoCommand':
      assert.equal(typeof message.data, 'object');
      return;
    case 'MeshEditCommand':
      assert.equal(typeof message.data, 'object');
      return;
    case 'StartDiffusion':
      assert.equal(typeof message.data.prompt, 'string');
      assert.equal(typeof message.data.guidance_scale, 'number');
      return;
    default:
      throw new Error(`Unhandled UiToBevy sample type: ${message.type}`);
  }
}

test('rust ipc samples cover the active frontend contract surface', () => {
  const samples = loadSamples();

  assert.ok(Array.isArray(samples.bevy_to_ui));
  assert.ok(Array.isArray(samples.ui_to_bevy));

  const inboundTypes = new Set(samples.bevy_to_ui.map((message) => message.type));
  const outboundTypes = new Set(samples.ui_to_bevy.map((message) => message.type));

  assert.ok(inboundTypes.has('ShowAddObjectMenu'));
  assert.ok(inboundTypes.has('LayerStateChanged'));
  assert.ok(inboundTypes.has('MeshEditModeChanged'));
  assert.ok(outboundTypes.has('UpdateLighting'));
  assert.ok(outboundTypes.has('SetDepthView'));
  assert.ok(outboundTypes.has('PaintCommand'));
  assert.ok(outboundTypes.has('SculptCommand'));
  assert.ok(inboundTypes.has('PaintBrushStateChanged'));
  assert.ok(inboundTypes.has('SculptBrushStateChanged'));
  assert.ok(inboundTypes.has('SculptHistoryChanged'));
});

test('rust ipc samples satisfy the JavaScript consumer expectations', () => {
  const samples = loadSamples();

  samples.bevy_to_ui.forEach(assertBevyToUiMessage);
  samples.ui_to_bevy.forEach(assertUiToBevyMessage);
});

test('native keyboard samples preserve physical keys, native text, and additive modifiers', () => {
  const { native_keyboard } = loadSamples();
  assert.deepEqual(native_keyboard.map(({ key, code, text }) => ({ key, code, text })), [
    { key: '3', code: 'Digit3', text: '#' },
    { key: 'Enter', code: 'Enter', text: null }
  ]);
  for (const event of native_keyboard) {
    assert.equal(event.pressed, true);
    for (const name of ['shift', 'ctrl', 'alt', 'meta', 'alt_graph']) assert.equal(typeof event.modifiers[name], 'boolean');
  }
  assert.equal(native_keyboard[0].modifiers.shift, true);
});

 test('color sampling commands carry explicit native source and enabled state', () => {
  const samples = loadSamples();
  assert.ok(samples.bevy_to_ui.some(m => m.type === 'PaintColorSamplingChanged'));
  const paint = samples.ui_to_bevy.filter(m => m.type === 'PaintCommand').map(m => m.data);
  assert.deepEqual(paint.find(m => m.SetColorSampling), { SetColorSampling: { enabled: true } });
  assert.deepEqual(paint.find(m => m.SetColorSampleSource), { SetColorSampleSource: { source: 'ActiveLayer' } });
 });

 test('DirectUV shares paint commands and reports snapshot payload bounds', () => {
  const samples = loadSamples();
  const commands = samples.ui_to_bevy.filter(m => m.type === 'PaintCommand').map(m => m.data);
  assert.ok(commands.includes('CancelStroke'));
  for (const target of ['Canvas', 'DirectUv']) assert.ok(commands.some(m => m?.SetTarget?.target === target));
  const target = samples.bevy_to_ui.find(m => m.type === 'PaintBrushStateChanged').data.target;
  assert.equal(target.mode, 'DirectUv');
  assert.equal(target.limit_bytes, 64 * 1024 * 1024);
  assert.ok(target.retained_bytes <= target.limit_bytes);
  assert.ok(target.pending_bytes <= 32 * 1024 * 1024);
 });

 test('shared UV protocol keeps selected receiver and layers separate from Canvas source history', () => {
  const samples=loadSamples();
  const commands=samples.ui_to_bevy.filter(m=>m.type==='PaintCommand' && m.data?.UvLayers).map(m=>m.data.UvLayers.command);
  assert.ok(commands.includes('Enable'));assert.ok(commands.includes('Undo'));assert.ok(commands.includes('Redo'));
  assert.ok(commands.some(c=>c.SelectReceiver?.mesh_id===12));assert.ok(commands.some(c=>c.Select?.layer_id===3));
  const state=samples.bevy_to_ui.find(m=>m.type==='PaintBrushStateChanged').data.target.uv_layers;
  assert.equal(state.receiver,12);assert.equal(state.enabled,true);assert.equal(state.layers[0].is_active,true);
  assert.equal(state.layers[0].locked,false);assert.equal(state.layers[0].opacity,0.5);
  assert.equal(state.layers[0].blend_mode,'Overlay');
  assert.ok(commands.some(c=>c.BlendMode?.layer_id===3 && c.BlendMode.mode==='Multiply'));
  assert.equal(state.can_undo,true);assert.equal(state.can_redo,false);assert.equal(state.conflicted,false);
 });

 test('live UV preview cancellation is distinct from source stroke cancellation',()=>{
   const samples=loadSamples();const commands=samples.ui_to_bevy.filter(m=>m.type==='PaintCommand').map(m=>m.data);
   assert.ok(commands.includes('CancelStroke'));assert.ok(commands.includes('CancelUvProjection'));
   const state=samples.bevy_to_ui.find(m=>m.type==='PaintBrushStateChanged').data.target.uv_layers;
   assert.equal(state.projection_preview,false);
 });
