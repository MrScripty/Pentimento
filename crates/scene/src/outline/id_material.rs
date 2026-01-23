//! Entity ID material for the ID pass
//!
//! This material renders entities with their ID encoded as a color,
//! allowing the edge detection shader to find object boundaries.

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// Uniform data for the entity ID shader
#[derive(Clone, Copy, ShaderType, Default)]
pub struct EntityIdUniform {
    /// Entity ID encoded as normalized RGBA
    /// R = low 8 bits, G = mid 8 bits, B = high 8 bits, A = 1.0
    pub entity_color: Vec4,
}

/// Material that outputs entity ID as color instead of PBR shading
#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
pub struct EntityIdMaterial {
    #[uniform(0)]
    pub entity_id: EntityIdUniform,
}

impl Material for EntityIdMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://pentimento_scene/outline/shaders/entity_id.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
}

/// Encode an entity's index as a color
pub fn entity_to_color(entity: Entity) -> Vec4 {
    let id: u32 = entity.index_u32();
    // Encode 24-bit entity index into RGB channels
    // Each channel gets 8 bits (0-255 range normalized to 0.0-1.0)
    let r = ((id & 0xFF) as f32) / 255.0;
    let g = (((id >> 8) & 0xFF) as f32) / 255.0;
    let b = (((id >> 16) & 0xFF) as f32) / 255.0;
    Vec4::new(r, g, b, 1.0)
}

/// Decode a color back to entity index (for debugging)
#[allow(dead_code)]
pub fn color_to_entity_index(color: Vec4) -> u32 {
    let r = (color.x * 255.0).round() as u32;
    let g = (color.y * 255.0).round() as u32;
    let b = (color.z * 255.0).round() as u32;
    r | (g << 8) | (b << 16)
}

/// Component marking an entity for ID buffer rendering
#[derive(Component)]
pub struct RenderToIdBuffer {
    /// The entity's ID color
    pub entity_color: Vec4,
}
