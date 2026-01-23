// Edge detection shader for Surface ID outline rendering
// Detects boundaries between different entity IDs and outputs orange outline

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

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let texel_size = 1.0 / uniforms.texture_size;
    let center_id = textureSample(id_texture, id_sampler, in.uv);

    // Early exit: if center pixel has no ID (black/transparent), no outline
    if center_id.a < 0.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Sample neighbors based on thickness
    let offset = texel_size * uniforms.thickness;

    // 4-neighbor sampling (cross pattern) - efficient
    let up = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(0.0, -offset.y));
    let down = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(0.0, offset.y));
    let left = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(-offset.x, 0.0));
    let right = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(offset.x, 0.0));

    // Check if any neighbor has a different ID
    var is_edge = false;

    if !ids_match(center_id, up) {
        is_edge = true;
    }
    if !ids_match(center_id, down) {
        is_edge = true;
    }
    if !ids_match(center_id, left) {
        is_edge = true;
    }
    if !ids_match(center_id, right) {
        is_edge = true;
    }

    if is_edge {
        return uniforms.outline_color;
    } else {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}

// Compare two ID colors with tolerance for precision errors
fn ids_match(a: vec4<f32>, b: vec4<f32>) -> bool {
    // If one is background (alpha < 0.5) and other isn't, they don't match
    if (a.a < 0.5) != (b.a < 0.5) {
        return false;
    }
    // If both are background, they match (both "no entity")
    if a.a < 0.5 && b.a < 0.5 {
        return true;
    }
    // Compare RGB with small tolerance for floating point precision
    let diff = abs(a.rgb - b.rgb);
    let threshold = 1.0 / 512.0; // Half a step in 8-bit precision
    return all(diff < vec3<f32>(threshold));
}
