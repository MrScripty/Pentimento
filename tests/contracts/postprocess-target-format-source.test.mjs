// Structural wiring guards. Rust descriptor/ECS tests and real GPU tests are separate gates.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const passes = [
  ['EdgeDetection', 'outline/edge_detection.rs'],
  ['DepthView', 'depth_view/mod.rs'],
];
for (const [name, file] of passes) {
  const source = readFileSync(`crates/scene/src/${file}`, 'utf8');
  test(`${name} selects an exact-format variant on each view before ping-pong`, () => {
    assert.match(source, new RegExp(`impl SpecializedRenderPipeline for ${name}Pipeline \\{\\s*type Key = TextureFormat;`));
    assert.match(source, /let format = target.main_texture_format\(\);\s*let id = pipelines.specialize\(&pipeline_cache, &pipeline, format\);/);
    assert.match(source, new RegExp(`commands\\s*\\.entity\\(entity\\)\\s*\\.insert\\([\\s\\S]*${name}PreparedPipeline \\{ format, id \\}`));
    assert.match(source, /RenderSystems::PrepareBindGroups/);
    const node = source.slice(source.indexOf(`impl ViewNode for ${name}Node`), source.indexOf(`pub struct ${name}Pipeline`));
    assert.match(node, new RegExp(`Option<&'static ${name}PreparedPipeline>`));
    assert.ok(node.indexOf('matches_format(view_target.main_texture_format())') < node.indexOf('post_process_write()'));
    assert.ok(node.indexOf('get_render_pipeline(prepared_pipeline.id)') < node.indexOf('post_process_write()'));
    assert.doesNotMatch(source, /format: TextureFormat::Rgba16Float/);
  });
}
test('depth preparation is per-view and clears unavailable prepass data', () => {
  const source = readFileSync('crates/scene/src/depth_view/mod.rs', 'utf8');
  assert.match(source, /#\[derive\(Component\)\]\npub struct DepthViewPrepared/);
  assert.match(source, /remove::<\(DepthViewPrepared, DepthViewPreparedPipeline\)>/);
  assert.match(source, /for \(entity, target, prepass\) in &views/);
  assert.doesNotMatch(source, /views.iter\(\).next\(\)|get_resource::<DepthViewPrepared>/);
  assert.match(source, /multisampled: true/);
});
