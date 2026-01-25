//! Blender-style camera controller
//!
//! Controls:
//! - Middle mouse drag: Orbit around target
//! - Shift + Middle mouse drag: Pan
//! - Scroll wheel: Dolly (zoom)

use bevy::input::mouse::{MouseButton, MouseMotion, MouseWheel};
use bevy::prelude::*;

use crate::canvas_plane::ActiveCanvasPlane;

/// Marker component for the main camera
#[derive(Component)]
pub struct MainCamera;

/// Camera orbit controller state
#[derive(Component)]
pub struct OrbitCamera {
    /// Point the camera orbits around
    pub target: Vec3,
    /// Distance from target
    pub distance: f32,
    /// Horizontal angle (yaw) in radians
    pub yaw: f32,
    /// Vertical angle (pitch) in radians
    pub pitch: f32,
    /// Orbit sensitivity (radians per pixel)
    pub orbit_sensitivity: f32,
    /// Pan sensitivity (units per pixel, scaled by distance)
    pub pan_sensitivity: f32,
    /// Zoom sensitivity (distance units per scroll line)
    pub zoom_sensitivity: f32,
    /// Minimum distance from target
    pub min_distance: f32,
    /// Maximum distance from target
    pub max_distance: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        // Default position: (5, 5, 5) looking at origin
        // distance = sqrt(5^2 + 5^2 + 5^2) = sqrt(75) ≈ 8.66
        // yaw = atan2(5, 5) = 45 degrees = PI/4
        // pitch = asin(5 / 8.66) ≈ 35.26 degrees ≈ 0.615 radians
        Self {
            target: Vec3::ZERO,
            distance: 8.66,
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.615,
            orbit_sensitivity: 0.005,
            pan_sensitivity: 0.002,
            zoom_sensitivity: 1.0,
            min_distance: 0.5,
            max_distance: 200.0,
        }
    }
}

impl OrbitCamera {
    /// Calculate camera position from orbit parameters
    pub fn calculate_position(&self) -> Vec3 {
        // Spherical to Cartesian conversion
        // Note: pitch is angle from horizontal, yaw is angle around Y axis
        let horizontal_distance = self.distance * self.pitch.cos();
        let y = self.distance * self.pitch.sin();
        let x = horizontal_distance * self.yaw.sin();
        let z = horizontal_distance * self.yaw.cos();

        self.target + Vec3::new(x, y, z)
    }

    /// Reset to default view
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Plugin for Blender-style camera controls
pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        // Order systems to avoid MessageReader conflicts:
        // orbit and pan both read MouseMotion, so they must run sequentially
        app.add_systems(
            Update,
            (
                camera_orbit_system,
                camera_pan_system.after(camera_orbit_system),
                camera_zoom_system,
                update_camera_transform
                    .after(camera_orbit_system)
                    .after(camera_pan_system)
                    .after(camera_zoom_system),
            ),
        );
    }
}

/// Handle orbit (middle mouse drag without shift)
fn camera_orbit_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut motion_events: MessageReader<MouseMotion>,
    mut camera_query: Query<&mut OrbitCamera>,
    active_plane: Res<ActiveCanvasPlane>,
) {
    // Don't allow camera movement when locked to a canvas plane
    if active_plane.camera_locked {
        motion_events.clear();
        return;
    }

    // Only orbit on middle mouse without shift
    if !mouse_button.pressed(MouseButton::Middle) {
        motion_events.clear();
        return;
    }

    let shift_held =
        key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);

    if shift_held {
        motion_events.clear();
        return; // Pan mode, not orbit
    }

    let mut delta = Vec2::ZERO;
    for event in motion_events.read() {
        delta += event.delta;
    }

    if delta == Vec2::ZERO {
        return;
    }

    for mut orbit in camera_query.iter_mut() {
        // Horizontal movement rotates around Y axis (yaw)
        orbit.yaw -= delta.x * orbit.orbit_sensitivity;

        // Vertical movement changes pitch (elevation)
        orbit.pitch -= delta.y * orbit.orbit_sensitivity;

        // Clamp pitch to prevent flipping (just below straight up/down)
        orbit.pitch = orbit.pitch.clamp(-1.5, 1.5);
    }
}

/// Handle pan (shift + middle mouse drag)
fn camera_pan_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut motion_events: MessageReader<MouseMotion>,
    mut camera_query: Query<(&mut OrbitCamera, &Transform)>,
    active_plane: Res<ActiveCanvasPlane>,
) {
    // Don't allow camera movement when locked to a canvas plane
    if active_plane.camera_locked {
        motion_events.clear();
        return;
    }

    if !mouse_button.pressed(MouseButton::Middle) {
        motion_events.clear();
        return;
    }

    let shift_held =
        key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);

    if !shift_held {
        motion_events.clear();
        return; // Orbit mode, not pan
    }

    let mut delta = Vec2::ZERO;
    for event in motion_events.read() {
        delta += event.delta;
    }

    if delta == Vec2::ZERO {
        return;
    }

    for (mut orbit, transform) in camera_query.iter_mut() {
        // Pan in camera's local XY plane
        let right = transform.rotation * Vec3::X;
        let up = transform.rotation * Vec3::Y;

        // Scale pan by distance so it feels consistent at different zoom levels
        let pan_scale = orbit.pan_sensitivity * orbit.distance;

        // Move target (negative to feel like dragging the scene)
        let pan_offset = (-right * delta.x + up * delta.y) * pan_scale;
        orbit.target += pan_offset;
    }
}

/// Handle zoom (scroll wheel)
fn camera_zoom_system(
    mut scroll_events: MessageReader<MouseWheel>,
    mut camera_query: Query<&mut OrbitCamera>,
    active_plane: Res<ActiveCanvasPlane>,
) {
    // Don't allow camera movement when locked to a canvas plane
    if active_plane.camera_locked {
        scroll_events.clear();
        return;
    }

    let mut scroll_delta = 0.0;
    for event in scroll_events.read() {
        scroll_delta += event.y;
    }

    if scroll_delta == 0.0 {
        return;
    }

    for mut orbit in camera_query.iter_mut() {
        // Zoom by adjusting distance (scroll up = zoom in = decrease distance)
        // Scale zoom speed by current distance for consistent feel
        let zoom_amount = scroll_delta * orbit.zoom_sensitivity * (orbit.distance * 0.1);
        orbit.distance -= zoom_amount;
        orbit.distance = orbit.distance.clamp(orbit.min_distance, orbit.max_distance);
    }
}

/// Update camera transform from orbit state
fn update_camera_transform(
    mut camera_query: Query<(&OrbitCamera, &mut Transform), With<MainCamera>>,
) {
    for (orbit, mut transform) in camera_query.iter_mut() {
        let position = orbit.calculate_position();
        *transform = Transform::from_translation(position).looking_at(orbit.target, Vec3::Y);
    }
}
