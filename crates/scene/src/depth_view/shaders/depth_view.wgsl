// Depth view visualization shader
// Reads the depth buffer and outputs linearized greyscale
// Gradient is auto-normalized to the scene's depth range

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct DepthViewUniform {
    near_plane: f32,
    scene_near: f32,
    scene_far: f32,
    _padding0: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: DepthViewUniform;

@group(0) @binding(1)
var depth_texture: texture_depth_multisampled_2d;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let tex_size = vec2<f32>(textureDimensions(depth_texture));
    let tex_coord = vec2<i32>(in.uv * tex_size);

    // Load raw reverse-Z depth (sample 0 for multisampled)
    // In Bevy's infinite reverse-Z: 1.0 = near plane, 0.0 = infinity
    let raw_depth = textureLoad(depth_texture, tex_coord, 0);

    // Linearize depth: for infinite reverse-Z, view_z = near / raw_depth
    var linear_depth: f32;
    if (raw_depth <= 0.0001) {
        // Sky / infinity — map to farthest scene depth
        linear_depth = uniforms.scene_far;
    } else {
        linear_depth = uniforms.near_plane / raw_depth;
    }

    // Normalize to scene depth range: scene_near → white, scene_far → black
    let depth_range = max(uniforms.scene_far - uniforms.scene_near, 0.001);
    let normalized = 1.0 - saturate((linear_depth - uniforms.scene_near) / depth_range);

    return vec4<f32>(normalized, normalized, normalized, 1.0);
}
