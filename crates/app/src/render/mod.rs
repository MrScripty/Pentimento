//! Render pipeline extensions for UI compositing

use bevy::prelude::*;

mod ui_composite;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        // Initialize window size tracking resource
        app.init_resource::<ui_composite::LastWindowSize>();

        // Use exclusive system for setup since it needs direct World access
        app.add_systems(Startup, ui_composite::setup_ui_composite)
            .add_systems(Update, ui_composite::update_ui_texture)
            .add_systems(Update, ui_composite::handle_window_resize);

        info!("Render plugin initialized with UI compositing");
    }
}
