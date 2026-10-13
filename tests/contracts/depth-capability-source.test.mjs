// Structural guard only: GPU and Rust behavior tests are separate qualification gates.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const source = readFileSync('crates/scene/src/depth_view/mod.rs', 'utf8');

test('unsupported backend returns before pipeline or depth-only systems are installed', () => {
  const build = source.slice(source.indexOf('fn build('), source.indexOf('fn finish('));
  const finish = source.slice(source.indexOf('fn finish('), source.indexOf('fn sync_depth_prepass('));
  assert.match(finish, /get_resource::<RenderAdapterInfo>/);
  assert.match(source, /Some\(wgpu::Backend::Gl\) =>/);
  assert.doesNotMatch(build, /init_(?:gpu_)?resource::<(?:DepthViewPipeline|SpecializedRenderPipelines<DepthViewPipeline>)>|prepare_depth_view\.in_set|(?:compute_scene_depth_bounds|sync_depth_prepass|toggle_expensive_features),/);
  const guard = finish.indexOf('if !supported');
  assert.ok(guard >= 0, 'unsupported-backend guard must be present');
  for (const setup of ['compute_scene_depth_bounds,', 'sync_depth_prepass,', 'toggle_expensive_features,', 'prepare_depth_view.in_set', 'init_gpu_resource::<DepthViewPipeline>', 'init_gpu_resource::<SpecializedRenderPipelines<DepthViewPipeline>>']) {
    assert.ok(finish.indexOf(setup) > guard);
  }
  assert.match(finish.slice(guard), /if !supported \{\s*return;/);
});

test('per-view depth schedule and outline ordering survive unavailable depth view', () => {
  const build = source.slice(source.indexOf('fn build('), source.indexOf('fn finish('));
  // Bevy 20 replaces render graph nodes with systems in each view's Core3d schedule.
  // Registration must remain unconditional; unsupported depth still orders outlines.
  assert.match(build, /render_app\.add_systems\(\s*Core3d,\s*render_depth_view\s*\.in_set\(Core3dSystems::PostProcess\)\s*\.in_set\(DepthViewLabel\)\s*\.after\(tonemapping\)/);
  assert.doesNotMatch(build, /if !supported/);
  const edge = readFileSync('crates/scene/src/outline/edge_detection.rs', 'utf8');
  assert.match(edge, /render_app\.add_systems\(\s*Core3d,\s*render_edge_detection\s*\.in_set\(Core3dSystems::PostProcess\)\s*\.in_set\(EdgeDetectionLabel\)\s*\.after\(crate::DepthViewLabel\)/);
  const render = source.slice(source.indexOf('fn render_depth_view('), source.indexOf('pub struct DepthViewPipeline'));
  assert.match(render, /pipeline: Option<Res<DepthViewPipeline>>/);
  assert.match(render, /let \(Some\(settings\), Some\(pipeline\)\) = \(settings, pipeline\) else \{\s*return;/);
});

test('WASM and native use the same admission helper and state query', () => {
  for (const path of ['crates/app-wasm/src/lib.rs', 'crates/app/src/render/ui_commands.rs']) {
    const handler = readFileSync(path, 'utf8');
    assert.match(handler, /UiToBevy::GetDepthViewState/);
    assert.match(handler, /capability.set_enabled\(&mut settings, enabled\)/);
    assert.match(handler, /capability.state_message\(&settings\)/);
  }
});
