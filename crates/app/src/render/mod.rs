//! Render pipeline extensions for UI compositing

use bevy::prelude::*;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, _app: &mut App) {
        // TODO: Add UI composite render node
        // This will be implemented in Phase 4
        info!("Render plugin initialized (UI compositing pending)");
    }
}
