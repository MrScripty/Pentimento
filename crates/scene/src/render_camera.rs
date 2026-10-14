//! Render camera for defining maximum mesh density during sculpting
//!
//! The render camera is a scene object (separate from the user viewport) that
//! defines the final render resolution. During sculpting, this camera is used
//! to calculate the maximum allowed mesh density (2 vertices per pixel max).
//!
//! Users can select the render camera and press Tab to look through it,
//! similar to how canvas planes work.

use bevy::math::Mat4;
use bevy::prelude::*;

use crate::frontend_input::FrontendInputBlockState;

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
        bevy::math::proj::perspective(self.fov, self.aspect_ratio(), self.near, self.far)
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
    input_blocks: Res<FrontendInputBlockState>,
    mut active_render_camera: ResMut<ActiveRenderCamera>,
) {
    if input_blocks.blocks_keyboard() {
        return;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_preserves_right_handed_zero_to_one_depth() {
        for camera in [
            RenderCamera::default(),
            RenderCamera::with_resolution(800, 1200),
            RenderCamera {
                fov: 0.75,
                near: 0.5,
                far: 250.0,
                ..Default::default()
            },
        ] {
            // The previous Mat4 constructor's formula, without relying on its
            // deprecated API. The supported constructor may regroup arithmetic.
            let (sin_fov, cos_fov) = (0.5 * camera.fov).sin_cos();
            let h = cos_fov / sin_fov;
            let r = camera.far / (camera.near - camera.far);
            let previous = Mat4::from_cols(
                Vec4::new(h / camera.aspect_ratio(), 0.0, 0.0, 0.0),
                Vec4::new(0.0, h, 0.0, 0.0),
                Vec4::new(0.0, 0.0, r, -1.0),
                Vec4::new(0.0, 0.0, r * camera.near, 0.0),
            );
            let projection = camera.projection_matrix();
            assert!(projection.abs_diff_eq(previous, 1.0e-6));
            let near = projection.project_point3(Vec3::new(0.0, 0.0, -camera.near));
            let far = projection.project_point3(Vec3::new(0.0, 0.0, -camera.far));
            assert!(near.abs_diff_eq(Vec3::ZERO, 1.0e-6));
            assert!(far.abs_diff_eq(Vec3::Z, 1.0e-6));
            assert!(projection.project_point3(Vec3::new(0.0, 0.1, -1.0)).y > 0.0);
        }
    }
}
