//! Outline configuration settings

use bevy::color::palettes::css::ORANGE;
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

/// Configuration for the selection outline effect
#[derive(Resource, Clone, ExtractResource)]
pub struct OutlineSettings {
    /// Outline color
    pub color: LinearRgba,
    /// Outline thickness in pixels (1-5 recommended)
    pub thickness: f32,
    /// Whether outlines are enabled
    pub enabled: bool,
}

impl Default for OutlineSettings {
    fn default() -> Self {
        Self {
            color: Color::from(ORANGE).to_linear(),
            thickness: 2.0,
            enabled: true,
        }
    }
}
