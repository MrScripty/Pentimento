//! Render camera for defining maximum mesh density during sculpting
//!
//! The render camera is a scene object (separate from the user viewport) that
//! defines the final render resolution. During sculpting, this camera is used
//! to calculate the maximum allowed mesh density (2 vertices per pixel max).
//!
//! Users can select the render camera and press Tab to look through it,
//! similar to how canvas planes work.

use bevy::prelude::*;

/// Component marking a camera as the render camera for density calculations.
///
/// This camera defines the maximum mesh detail allowed during sculpting.
/// The constraint is: maximum 2 vertices per pixel as seen from this camera.
#[derive(Component, Clone)]
pub struct RenderCamera {
    /// Render resolution (default 1920×1080)
    pub resolution: UVec2,
    /// Field of view in radians (default 60 degrees)
    pub fov: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
}

impl Default for RenderCamera {
    fn default() -> Self {
        Self {
            resolution: UVec2::new(1920, 1080),
            fov: std::f32::consts::FRAC_PI_3, // 60 degrees
            near: 0.1,
            far: 1000.0,
        }
    }
}

impl RenderCamera {
    /// Create a render camera with custom resolution
    pub fn with_resolution(width: u32, height: u32) -> Self {
        Self {
            resolution: UVec2::new(width, height),
            ..Default::default()
        }
    }

    /// Get the aspect ratio
    pub fn aspect_ratio(&self) -> f32 {
        self.resolution.x as f32 / self.resolution.y as f32
    }

    /// Calculate the maximum allowed vertices for a mesh based on this camera's view.
    ///
    /// Uses the constraint: max 2 vertices per pixel visible from this camera.
    ///
    /// # Arguments
    /// * `visible_surface_area` - Surface area of mesh visible to camera (world units²)
    /// * `mesh_screen_area` - Approximate screen area covered by mesh (pixels²)
    ///
    /// # Returns
    /// Maximum number of vertices allowed
    pub fn max_vertices_for_mesh(&self, mesh_screen_area: f32) -> usize {
        // Max 2 vertices per pixel
        let max_vertices = (mesh_screen_area * 2.0) as usize;
        // Floor to prevent degenerate meshes
        max_vertices.max(100)
    }

    /// Total pixels in the render resolution
    pub fn total_pixels(&self) -> u32 {
        self.resolution.x * self.resolution.y
    }
}

/// Resource tracking the active render camera for density calculations
#[derive(Resource, Default)]
pub struct ActiveRenderCamera {
    /// The entity of the active render camera (if any)
    pub entity: Option<Entity>,
}

/// Plugin for render camera functionality
pub struct RenderCameraPlugin;

impl Plugin for RenderCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveRenderCamera>()
            .add_systems(Update, handle_render_camera_selection);
    }
}

/// Handle Tab key to look through selected render camera
#[cfg(feature = "selection")]
fn handle_render_camera_selection(
    key_input: Res<ButtonInput<KeyCode>>,
    selected_cameras: Query<Entity, (With<crate::selection::Selected>, With<RenderCamera>)>,
    mut active_render_camera: ResMut<ActiveRenderCamera>,
) {
    // Tab to activate selected render camera (without modifiers)
    let tab = key_input.just_pressed(KeyCode::Tab);
    let shift = key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);
    let ctrl = key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight);

    // Only plain Tab, not Shift+Tab or Ctrl+Tab
    if !tab || shift || ctrl {
        return;
    }

    if let Ok(entity) = selected_cameras.single() {
        if active_render_camera.entity == Some(entity) {
            // Already active, deactivate
            active_render_camera.entity = None;
            info!("Deactivated render camera view");
        } else {
            // Activate this camera
            active_render_camera.entity = Some(entity);
            info!("Activated render camera view for entity {:?}", entity);
        }
    }
}

/// Stub for non-selection builds
#[cfg(not(feature = "selection"))]
fn handle_render_camera_selection() {}
