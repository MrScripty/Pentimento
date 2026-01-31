//! Resource types for Dioxus UI rendering.

use bevy::prelude::*;
use bevy::asset::AssetId;
use bevy::render::extract_resource::ExtractResource;
use pentimento_dioxus_ui::Scene;

// ============================================================================
// Main World Resources
// ============================================================================

/// UI state that gets extracted to the render world each frame.
/// This contains viewport dimensions needed for Vello rendering.
#[derive(Resource, Clone, ExtractResource, Default)]
pub struct DioxusUiState {
    pub width: u32,
    pub height: u32,
}

/// Handle to the render target texture (extracted to render world via AssetId).
#[derive(Resource, Clone)]
pub struct DioxusRenderTarget {
    pub handle: Handle<Image>,
}

/// Extractable version that just carries the AssetId.
#[derive(Resource, Clone, ExtractResource)]
pub struct DioxusRenderTargetId(pub AssetId<Image>);

/// Marker component for the UI overlay node.
#[derive(Component)]
pub struct DioxusUiOverlay;

/// Pre-built Vello scene for the current frame.
/// Built in main world, extracted to render world.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct VelloSceneBuffer {
    pub scene: Scene,
}

// ============================================================================
// Status Resources
// ============================================================================

/// Track initialization status in render world.
#[derive(Resource, Default)]
pub struct VelloRenderStatus {
    pub first_render_done: bool,
}

/// Track whether Dioxus UI setup is complete (main world).
/// We defer setup to allow the window size to stabilize after creation.
#[derive(Resource, Default)]
pub struct DioxusSetupStatus {
    pub setup_done: bool,
    pub frames_waited: u32,
}
