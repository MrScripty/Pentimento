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
  assert.doesNotMatch(build, /init_resource::<DepthViewPipeline>|prepare_depth_view\.in_set|sync_depth_prepass,/);
  const guard = finish.indexOf('if !supported');
  for (const setup of ['sync_depth_prepass,', 'prepare_depth_view.in_set', 'init_resource::<DepthViewPipeline>']) {
    assert.ok(finish.indexOf(setup) > guard);
  }
  assert.match(finish.slice(guard), /if !supported \{\s*return;/);
});

test('depth graph label and outline ordering survive unavailable depth view', () => {
  assert.match(source, /add_render_graph_node::<ViewNodeRunner<DepthViewNode>>\(Core3d, DepthViewLabel\)/);
  assert.match(source, /Node3d::Tonemapping,\s*DepthViewLabel,\s*Node3d::EndMainPassPostProcessing/);
  const edge = readFileSync('crates/scene/src/outline/edge_detection.rs', 'utf8');
  assert.match(edge, /DepthViewLabel,\s*EdgeDetectionLabel/);
});

test('WASM and native use the same admission helper and state query', () => {
  for (const path of ['crates/app-wasm/src/lib.rs', 'crates/app/src/render/ui_commands.rs']) {
    const handler = readFileSync(path, 'utf8');
    assert.match(handler, /UiToBevy::GetDepthViewState/);
    assert.match(handler, /capability.set_enabled\(&mut settings, enabled\)/);
    assert.match(handler, /capability.state_message\(&settings\)/);
  }
});
