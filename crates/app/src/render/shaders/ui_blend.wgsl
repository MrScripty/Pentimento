// UI blend shader for transparent Dioxus/Vello overlay
// Samples the Vello-rendered texture and blends it over the 3D scene
// with proper alpha compositing.

#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var ui_texture: texture_2d<f32>;
@group(1) @binding(1) var ui_sampler: sampler;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(ui_texture, ui_sampler, in.uv);

    // The texture is in linear color space from Vello
    // Convert to sRGB for display
    let srgb = pow(color.rgb, vec3(1.0 / 2.2));

    // Output STRAIGHT alpha (not premultiplied)
    // Bevy's UiMaterial uses BlendState::ALPHA_BLENDING which expects straight alpha
    // Blend formula: output = src.rgb * src.a + dst.rgb * (1 - src.a)
    return vec4(srgb, color.a);
}
