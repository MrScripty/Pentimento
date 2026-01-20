//! Application configuration and compositing mode selection

use bevy::prelude::*;

/// Compositing mode for combining 3D scene with UI overlay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Resource)]
pub enum CompositeMode {
    /// Offscreen capture with texture upload (default, most compatible)
    /// Works on all display servers but has higher CPU usage
    #[default]
    Capture,
    /// Transparent child window overlay (better performance)
    /// Uses wry's native windowing, may have issues on some systems
    Overlay,
}

impl CompositeMode {
    /// Parse from environment variable PENTIMENTO_COMPOSITE
    pub fn from_env() -> Self {
        match std::env::var("PENTIMENTO_COMPOSITE").as_deref() {
            Ok("overlay") => Self::Overlay,
            Ok("capture") | _ => Self::Capture,
        }
    }
}

/// Application configuration resource
#[derive(Resource, Clone)]
pub struct PentimentoConfig {
    pub composite_mode: CompositeMode,
}

impl Default for PentimentoConfig {
    fn default() -> Self {
        Self {
            composite_mode: CompositeMode::from_env(),
        }
    }
}
