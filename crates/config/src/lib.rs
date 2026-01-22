//! Shared configuration for Pentimento
//!
//! This crate provides the single source of truth for window dimensions,
//! display settings, and other configuration shared across all build modes
//! (native Bevy, Tauri/WASM).

use serde::{Deserialize, Serialize};

#[cfg(feature = "bevy")]
use bevy::prelude::Resource;

/// Default window width in pixels
pub const DEFAULT_WIDTH: u32 = 1920;

/// Default window height in pixels
pub const DEFAULT_HEIGHT: u32 = 1080;

/// Default scale factor (1.0 = no scaling)
pub const DEFAULT_SCALE: f32 = 1.0;

/// Display configuration for window and rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bevy", derive(Resource))]
pub struct DisplayConfig {
    /// Window width in logical pixels
    pub width: u32,
    /// Window height in logical pixels
    pub height: u32,
    /// Scale factor for DPI scaling
    pub scale: f32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            scale: DEFAULT_SCALE,
        }
    }
}

impl DisplayConfig {
    /// Create a new display config with the given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            scale: DEFAULT_SCALE,
        }
    }

    /// Get width as f32 for calculations
    pub fn width_f32(&self) -> f32 {
        self.width as f32
    }

    /// Get height as f32 for calculations
    pub fn height_f32(&self) -> f32 {
        self.height as f32
    }

    /// Get scaled width (for physical pixel calculations)
    pub fn scaled_width(&self) -> u32 {
        (self.width as f32 * self.scale) as u32
    }

    /// Get scaled height (for physical pixel calculations)
    pub fn scaled_height(&self) -> u32 {
        (self.height as f32 * self.scale) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DisplayConfig::default();
        assert_eq!(config.width, DEFAULT_WIDTH);
        assert_eq!(config.height, DEFAULT_HEIGHT);
        assert_eq!(config.scale, DEFAULT_SCALE);
    }

    #[test]
    fn test_scaled_dimensions() {
        let mut config = DisplayConfig::default();
        config.scale = 2.0;
        assert_eq!(config.scaled_width(), 3840);
        assert_eq!(config.scaled_height(), 2160);
    }
}
