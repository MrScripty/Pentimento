//! Custom UI material for alpha-blended Dioxus/Vello overlay
//!
//! This material properly samples the Vello-rendered texture (linear color space)
//! and blends it over the 3D scene with correct alpha compositing.

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// Plugin to register the UI blend material
pub struct UiBlendMaterialPlugin;

impl Plugin for UiBlendMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/ui_blend.wgsl");
        app.add_plugins(UiMaterialPlugin::<UiBlendMaterial>::default());
        info!("UiBlendMaterial plugin registered");
    }
}

/// Custom UI material that samples a texture with proper alpha blending
///
/// This is used for the Dioxus/Vello UI overlay, which renders to a linear
/// color space texture that needs to be composited over the 3D scene.
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct UiBlendMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
}

impl UiMaterial for UiBlendMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://pentimento/render/shaders/ui_blend.wgsl".into()
    }
}
