// Entity ID shader - outputs entity ID as fragment color
// Used for the ID pass in Surface ID outline rendering

#import bevy_pbr::forward_io::VertexOutput

struct EntityIdUniform {
    entity_color: vec4<f32>,
}

@group(2) @binding(0)
var<uniform> entity_id: EntityIdUniform;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Output the entity ID as the fragment color
    // No lighting, no PBR - just the raw ID encoded as RGB
    return entity_id.entity_color;
}
