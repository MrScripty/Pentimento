// Edge detection shader for Surface ID outline rendering
// Composites outlines onto the scene using ViewTarget post-processing pattern
//
// To prevent feedback loop (erosion/dilation), we:
// 1. Only draw outlines on pixels that are part of the selected object (have ID)
// 2. Only draw if the scene pixel doesn't already look like the outline color
// This ensures outlines are drawn exactly once and don't accumulate.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EdgeDetectionUniform {
    outline_color: vec4<f32>,
    thickness: f32,
    texture_size: vec2<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: EdgeDetectionUniform;

@group(0) @binding(1)
var id_texture: texture_2d<f32>;

@group(0) @binding(2)
var id_sampler: sampler;

@group(0) @binding(3)
var scene_texture: texture_2d<f32>;

@group(0) @binding(4)
var scene_sampler: sampler;

// Sample the ID at a pixel offset
fn sample_id(uv: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;
    return textureSample(id_texture, id_sampler, uv + offset * pixel_size);
}

// Check if a color is similar to the outline color (already has outline)
fn is_outline_color(color: vec4<f32>) -> bool {
    let diff = abs(color.rgb - uniforms.outline_color.rgb);
    let threshold = 0.1;
    return diff.r < threshold && diff.g < threshold && diff.b < threshold;
}

// Check if the current pixel is on the edge boundary in the ID buffer
// Returns true if center pixel has ID and any neighbor does NOT have ID
fn is_boundary_edge(uv: vec2<f32>) -> bool {
    let center_id = sample_id(uv, vec2<f32>(0.0, 0.0));

    // If center pixel has no ID, it's not part of a selected object
    if center_id.a < 0.01 {
        return false;
    }

    let thickness = uniforms.thickness;

    // Check neighbors in 8 directions
    let offsets = array<vec2<f32>, 8>(
        vec2<f32>(-thickness, 0.0),         // left
        vec2<f32>(thickness, 0.0),          // right
        vec2<f32>(0.0, -thickness),         // up
        vec2<f32>(0.0, thickness),          // down
        vec2<f32>(-thickness, -thickness),  // top-left
        vec2<f32>(thickness, -thickness),   // top-right
        vec2<f32>(-thickness, thickness),   // bottom-left
        vec2<f32>(thickness, thickness),    // bottom-right
    );

    for (var i = 0; i < 8; i++) {
        let neighbor_id = sample_id(uv, offsets[i]);

        // If any neighbor has NO ID, this is a boundary pixel
        if neighbor_id.a < 0.01 {
            return true;
        }
    }

    return false;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the scene
    let scene_color = textureSample(scene_texture, scene_sampler, in.uv);

    // Check if this pixel is on the edge of a selected object
    if is_boundary_edge(in.uv) {
        // Only draw outline if scene pixel doesn't already have the outline color
        // This prevents feedback loop accumulation
        if !is_outline_color(scene_color) {
            return uniforms.outline_color;
        }
    }

    // Pass through the scene unchanged
    return scene_color;
}
