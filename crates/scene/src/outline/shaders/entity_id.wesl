// Entity ID shader - outputs entity ID as fragment color
// Used for the ID pass in Surface ID outline rendering

import bevy_pbr::render::forward_io::VertexOutput;

struct EntityIdUniform {
    entity_color: vec4<f32>,
}

// Bevy supplies the material bind group through its WESL constants module.
@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> entity_id: EntityIdUniform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Output the entity ID as the fragment color
    // No lighting, no PBR - just the raw ID encoded as RGB
    return entity_id.entity_color;
}
