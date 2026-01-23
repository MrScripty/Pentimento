// Edge detection shader for Surface ID outline rendering
// Outputs outline color at edges, transparent elsewhere (for alpha blending)
// Only reads from ID buffer - no scene sampling to avoid feedback loops

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
    let center_id = textureSample(id_texture, id_sampler, in.uv);

    // If center is part of selected object, output transparent (preserve scene underneath)
    if center_id.a >= 0.5 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Center is background - check if any neighbor is a selected object
    let texel_size = 1.0 / uniforms.texture_size;
    let offset = texel_size * uniforms.thickness;

    // 4-neighbor sampling (cross pattern)
    let up = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(0.0, -offset.y));
    let down = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(0.0, offset.y));
    let left = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(-offset.x, 0.0));
    let right = textureSample(id_texture, id_sampler, in.uv + vec2<f32>(offset.x, 0.0));

    // If any neighbor is a selected object, this is an exterior edge - draw outline
    if up.a >= 0.5 || down.a >= 0.5 || left.a >= 0.5 || right.a >= 0.5 {
        return uniforms.outline_color;
    }

    // Not near any selected object - output transparent
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}

