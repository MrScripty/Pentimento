// Structural wiring guards. Rust descriptor/ECS tests and real GPU tests are separate gates.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const passes = [
  ['EdgeDetection', 'edge_detection', 'OutlineCamera', 'outline/edge_detection.rs'],
  ['DepthView', 'depth_view', 'DepthViewCamera', 'depth_view/mod.rs'],
];
for (const [name, system, camera, file] of passes) {
  const source = readFileSync(`crates/scene/src/${file}`, 'utf8');
  test(`${name} selects an exact-format variant on each view before ping-pong`, () => {
    assert.match(source, new RegExp(`impl SpecializedRenderPipeline for ${name}Pipeline \\{\\s*type Key = TextureFormat;`));
    assert.match(source, /let format = target.main_texture_format\(\);\s*let id = pipelines.specialize\(&pipeline_cache, &pipeline, format\);/);
    assert.match(source, new RegExp(`commands\\s*\\.entity\\(entity\\)\\s*\\.insert\\([\\s\\S]*${name}PreparedPipeline \\{ format, id \\}`));
    assert.match(source, /RenderSystems::PrepareBindGroups/);
    const start = source.indexOf(`fn render_${system}(`);
    const end = source.indexOf(`pub struct ${name}Pipeline`);
    assert.ok(start >= 0 && end > start, 'per-view render system must be present');
    const render = source.slice(start, end);
    assert.match(render, /view: ViewQuery</);
    assert.match(render, new RegExp(`Option<&${name}PreparedPipeline>`));
    assert.match(render, new RegExp(`With<${camera}>`));
    assert.match(render, /view\.into_inner\(\)/);
    assert.match(render, /if !prepared_pipeline\.matches_format\(target\.main_texture_format\(\)\) \{\s*return;/);
    assert.match(render, /let Some\(render_pipeline\) = pipeline_cache\.get_render_pipeline\(prepared_pipeline\.id\) else \{\s*return;/);
    const write = render.indexOf('post_process_write()');
    assert.ok(write >= 0, 'render system must use per-view ping-pong');
    for (const guard of ['matches_format(target.main_texture_format())', 'get_render_pipeline(prepared_pipeline.id)']) {
      const position = render.indexOf(guard);
      assert.ok(position >= 0 && position < write, `${guard} must precede ping-pong`);
    }
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
