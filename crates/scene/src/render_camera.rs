//! Render camera for defining maximum mesh density during sculpting
//!
//! The render camera is a scene object (separate from the user viewport) that
//! defines the final render resolution. During sculpting, this camera is used
//! to calculate the maximum allowed mesh density (2 vertices per pixel max).
//!
//! Users can select the render camera and press Tab to look through it,
//! similar to how canvas planes work.

use bevy::prelude::*;
use bevy::math::Mat4;

/// Component marking a camera as the render camera for density calculations.
///
/// This camera defines the maximum mesh detail allowed during sculpting.
/// The vertex budget is derived from pixel coverage: `pixels * vertices_per_pixel`.
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
    /// Vertices allowed per pixel of coverage (default: 1.0)
    pub vertices_per_pixel: f32,
}

impl Default for RenderCamera {
    fn default() -> Self {
        Self {
            resolution: UVec2::new(1920, 1080),
            fov: std::f32::consts::FRAC_PI_3, // 60 degrees
            near: 0.1,
            far: 1000.0,
            vertices_per_pixel: 1.0,
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

    /// Calculate the maximum allowed vertices for a mesh based on pixel coverage.
    ///
    /// Uses the configured `vertices_per_pixel` multiplier.
    ///
    /// # Arguments
    /// * `pixel_coverage` - Number of pixels the mesh covers on screen from this camera
    ///
    /// # Returns
    /// Maximum number of vertices allowed
    pub fn max_vertices_for_mesh(&self, pixel_coverage: f32) -> usize {
        let max_vertices = (pixel_coverage * self.vertices_per_pixel) as usize;
        // Floor to prevent degenerate meshes
        max_vertices.max(100)
    }

    /// Total pixels in the render resolution
    pub fn total_pixels(&self) -> u32 {
        self.resolution.x * self.resolution.y
    }

    /// Build the projection matrix for this render camera.
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov, self.aspect_ratio(), self.near, self.far)
    }

    /// Build the view-projection matrix given the camera's world transform.
    pub fn view_projection(&self, camera_transform: &GlobalTransform) -> Mat4 {
        let view = camera_transform.affine().inverse();
        self.projection_matrix() * Mat4::from(view)
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
