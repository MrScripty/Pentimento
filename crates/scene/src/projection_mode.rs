//! Projection painting mode - projects 2D canvas paint onto 3D scene geometry.
//!
//! This module provides:
//! - [`ProjectionMode`] resource for tracking projection state
//! - [`ProjectionTarget`] component for meshes that can receive projected paint
//! - [`ProjectionEvent`] messages for projection operations
//!
//! Two projection modes are supported:
//! 1. **Paint-then-Project**: User paints on canvas, then clicks "Project to Scene"
//! 2. **Live Projection**: Paint projects to meshes in real-time as user strokes

use bevy::ecs::message::Message;
use bevy::prelude::*;

use painting::MeshStorageMode;

/// Resource tracking projection painting state
#[derive(Resource, Default)]
pub struct ProjectionMode {
    /// Whether projection mode is enabled (canvas paints project to meshes)
    pub enabled: bool,
    /// Live projection: project as user paints (vs batch project on demand)
    pub live_projection: bool,
}

/// Component marking a mesh as a target for projection painting.
///
/// When projection occurs, rays are cast from the camera through the canvas
/// and paint is applied to meshes with this component.
#[derive(Component)]
pub struct ProjectionTarget {
    /// How paint is stored on this mesh (UV atlas or PTex)
    pub storage_mode: MeshStorageMode,
    /// Handle to the paint texture (created when first painted)
    pub texture_handle: Option<Handle<Image>>,
    /// Whether this target needs GPU texture upload
    pub dirty: bool,
}

impl ProjectionTarget {
    /// Create a new UV atlas projection target
    pub fn uv_atlas(resolution: (u32, u32)) -> Self {
        Self {
            storage_mode: MeshStorageMode::UvAtlas { resolution },
            texture_handle: None,
            dirty: false,
        }
    }

    /// Create a new PTex projection target
    pub fn ptex(face_resolution: u32) -> Self {
        Self {
            storage_mode: MeshStorageMode::Ptex { face_resolution },
            texture_handle: None,
            dirty: false,
        }
    }
}

/// Messages for projection painting operations
#[derive(Message, Debug, Clone)]
pub enum ProjectionEvent {
    /// Toggle live projection mode on/off
    SetLiveProjection { enabled: bool },
    /// Project current canvas contents to all visible meshes (one-shot)
    ProjectToScene,
    /// Clear projected paint from a specific mesh
    ClearProjection { mesh_entity: Entity },
    /// Clear all projected paint from all meshes
    ClearAllProjections,
}

/// Plugin for projection painting functionality
pub struct ProjectionModePlugin;

impl Plugin for ProjectionModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProjectionMode>()
            .add_message::<ProjectionEvent>()
            .add_systems(Update, handle_projection_events);
    }
}

/// Handle projection mode events
fn handle_projection_events(
    mut projection_mode: ResMut<ProjectionMode>,
    mut events: MessageReader<ProjectionEvent>,
) {
    for event in events.read() {
        match event {
            ProjectionEvent::SetLiveProjection { enabled } => {
                projection_mode.live_projection = *enabled;
                projection_mode.enabled = *enabled;
                info!(
                    "Live projection {}",
                    if *enabled { "enabled" } else { "disabled" }
                );
            }
            ProjectionEvent::ProjectToScene => {
                info!("Project to scene requested");
                // Actual projection is handled by the projection painting system
                // This event signals it should run a full projection pass
            }
            ProjectionEvent::ClearProjection { mesh_entity } => {
                info!("Clear projection for entity {:?}", mesh_entity);
                // TODO: Clear the projection target's surface
            }
            ProjectionEvent::ClearAllProjections => {
                info!("Clear all projections");
                // TODO: Clear all projection targets
            }
        }
    }
}
