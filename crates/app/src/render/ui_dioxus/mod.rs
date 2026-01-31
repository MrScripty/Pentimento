//! Dioxus UI Compositing - Zero-Copy GPU rendering with Vello via Blitz
//!
//! This module renders the Dioxus UI using Blitz for DOM/CSS/layout and Vello
//! for GPU rendering directly to a Bevy-owned texture, eliminating CPU-side copies.
//!
//! # Architecture
//!
//! 1. Main world: BlitzDocument manages Dioxus VirtualDom + Blitz DOM/layout
//! 2. Main world (Update): poll() processes state changes, paint_to_scene() builds Vello scene
//! 3. Extraction: Scene is cloned to render world
//! 4. Render world: Vello renders the scene directly to Bevy's GpuImage
//! 5. Bevy composites the texture over the 3D scene
//!
//! # Thread Safety
//!
//! BlitzDocument contains !Send types (VirtualDom), so it stays in main world.
//! Only the Scene (which is Clone+Send) is extracted to the render world.
//! Vello's Renderer is wrapped in `Arc<Mutex<...>>` for thread safety.

mod event_bridge;
mod ipc_handler;
mod render;
mod resources;
mod scene_builder;
mod setup;

use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::renderer::RenderDevice;
use bevy::render::{Render, RenderApp, RenderSystems};
use pentimento_dioxus_ui::SharedVelloRenderer;

use super::ui_blend_material::UiBlendMaterialPlugin;

// Re-export public types
pub use event_bridge::{
    BlitzDocumentResource, DioxusBridgeResource, DioxusEventReceiver, DioxusEventSender,
    DioxusRendererResource,
};
pub use render::RenderWorldVelloRenderer;
pub use resources::{
    DioxusRenderTarget, DioxusRenderTargetId, DioxusSetupStatus, DioxusUiOverlay, DioxusUiState,
    VelloRenderStatus, VelloSceneBuffer,
};

// Import systems for plugin registration
use ipc_handler::handle_ui_to_bevy_messages;
use render::render_vello_to_texture;
use scene_builder::{build_ui_scene, handle_window_resize};
use setup::deferred_setup_dioxus_texture;

/// Plugin for Dioxus UI rendering with zero-copy GPU integration.
pub struct DioxusRenderPlugin;

impl Plugin for DioxusRenderPlugin {
    fn build(&self, app: &mut App) {
        // Main world setup
        app.init_resource::<DioxusUiState>()
            .init_resource::<VelloSceneBuffer>()
            .init_resource::<DioxusSetupStatus>()
            .add_plugins(UiBlendMaterialPlugin)
            .add_plugins(ExtractResourcePlugin::<DioxusUiState>::default())
            .add_plugins(ExtractResourcePlugin::<DioxusRenderTargetId>::default())
            .add_plugins(ExtractResourcePlugin::<VelloSceneBuffer>::default())
            // Run setup during Update (not Startup) to allow window size to stabilize
            // IMPORTANT: handle_ui_to_bevy_messages MUST run before build_ui_scene so that
            // IPC messages (like ShowAddObjectMenu) are forwarded to the bridge and the
            // component state is updated BEFORE the scene is painted.
            .add_systems(
                Update,
                (
                    deferred_setup_dioxus_texture,
                    handle_ui_to_bevy_messages, // Process IPC messages first
                    build_ui_scene,             // Then build scene with updated state
                    handle_window_resize,
                )
                    .chain(),
            );

        // Render world setup
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("DioxusRenderPlugin: RenderApp not available, skipping render world setup");
            return;
        };

        render_app
            .init_resource::<VelloRenderStatus>()
            .add_systems(Render, render_vello_to_texture.in_set(RenderSystems::Render));
    }

    fn finish(&self, app: &mut App) {
        // Initialize Vello renderer AFTER RenderDevice is available
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        let render_device = render_app.world().resource::<RenderDevice>();
        match SharedVelloRenderer::new(render_device.wgpu_device()) {
            Ok(renderer) => {
                render_app.insert_resource(RenderWorldVelloRenderer { renderer });
                info!("Vello renderer initialized in render world (zero-copy mode)");
            }
            Err(e) => {
                error!("Failed to create Vello renderer in render world: {}", e);
            }
        }
    }
}
