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
      assert.equal(typeof message.data, 'object');
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
});

test('rust ipc samples satisfy the JavaScript consumer expectations', () => {
  const samples = loadSamples();

  samples.bevy_to_ui.forEach(assertBevyToUiMessage);
  samples.ui_to_bevy.forEach(assertUiToBevyMessage);
});
