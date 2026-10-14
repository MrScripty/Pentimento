// Entity ID shader - outputs entity ID as fragment color
// Used for the ID pass in Surface ID outline rendering

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_render::color_operations::srgb_to_linear_rgb;

struct EntityIdUniform {
    entity_color: vec4<f32>,
}

// Bevy supplies the material bind group through its WESL constants module.
@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> entity_id: EntityIdUniform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Compensate for the attachment's linear-to-sRGB write so stored bytes are
    // the original 24-bit ID. Writing the normalized ID directly loses high IDs.
    return vec4(srgb_to_linear_rgb(entity_id.entity_color.rgb), entity_id.entity_color.a);
}
