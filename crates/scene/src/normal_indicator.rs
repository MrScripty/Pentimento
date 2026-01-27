//! Surface normal indicator - displays mesh normal at cursor position.
//!
//! Shows a visual indicator of the surface normal wherever the cursor
//! intersects meshes in the scene. In paint mode, constrained to the active mesh.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::mesh_paint_mode::{ray_mesh_intersection, MeshPaintState, PaintableMesh};

/// Resource controlling the normal indicator visualization
#[derive(Resource)]
pub struct NormalIndicatorState {
    /// Whether the indicator is enabled
    pub enabled: bool,
    /// Color of the normal line (core)
    pub color: Color,
    /// Color of the outline for contrast against any background
    pub outline_color: Color,
    /// Length of the normal line in world units
    pub length: f32,
    /// Cached hit data from last frame
    pub current_hit: Option<NormalIndicatorHit>,
}

/// Cached hit information for rendering
#[derive(Clone)]
pub struct NormalIndicatorHit {
    /// World position of the intersection
    pub position: Vec3,
    /// Surface normal at the intersection (normalized)
    pub normal: Vec3,
}

impl Default for NormalIndicatorState {
    fn default() -> Self {
        Self {
            enabled: true,
            color: Color::srgb(0.5, 0.5, 0.5),       // Neutral grey
            outline_color: Color::srgb(0.0, 0.0, 0.0), // Black outline
            length: 0.25,                             // Short indicator
            current_hit: None,
        }
    }
}

/// System that raycasts from cursor to find mesh intersections
fn update_normal_indicator(
    mut indicator: ResMut<NormalIndicatorState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mesh_paint_state: Res<MeshPaintState>,
    // Query all meshes for normal mode
    all_meshes: Query<(Entity, &Mesh3d, &GlobalTransform)>,
    // Query paintable meshes for paint mode
    paintable_meshes: Query<(Entity, &Mesh3d, &GlobalTransform), With<PaintableMesh>>,
    meshes: Res<Assets<Mesh>>,
) {
    // Clear previous hit
    indicator.current_hit = None;

    // Skip if disabled
    if !indicator.enabled {
        return;
    }

    // Get cursor position
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    // Get camera for ray casting
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Create ray from cursor
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    // Determine which meshes to test based on mode
    let mut closest: Option<(Vec3, Vec3, f32)> = None; // (position, normal, distance)

    if let Some(active_entity) = mesh_paint_state.active_mesh {
        // Paint mode: only test the active mesh
        if let Ok((_, mesh_handle, transform)) = paintable_meshes.get(active_entity) {
            if let Some(mesh) = meshes.get(&mesh_handle.0) {
                if let Some(hit) = ray_mesh_intersection(&ray, mesh, transform) {
                    let distance = ray.origin.distance(hit.world_pos);
                    closest = Some((hit.world_pos, hit.normal, distance));
                }
            }
        }
    } else {
        // Normal mode: test all meshes
        for (_entity, mesh_handle, transform) in all_meshes.iter() {
            let Some(mesh) = meshes.get(&mesh_handle.0) else {
                continue;
            };

            if let Some(hit) = ray_mesh_intersection(&ray, mesh, transform) {
                let distance = ray.origin.distance(hit.world_pos);
                if closest.is_none() || distance < closest.as_ref().unwrap().2 {
                    closest = Some((hit.world_pos, hit.normal, distance));
                }
            }
        }
    }

    // Store the closest hit
    if let Some((position, normal, _distance)) = closest {
        indicator.current_hit = Some(NormalIndicatorHit { position, normal });
    }
}

/// System that renders the normal indicator as a gizmo line
/// Uses double-pass rendering for visibility against any background
fn render_normal_indicator(indicator: Res<NormalIndicatorState>, mut gizmos: Gizmos) {
    if !indicator.enabled {
        return;
    }

    if let Some(ref hit) = indicator.current_hit {
        let start = hit.position;
        let end = hit.position + hit.normal * indicator.length;

        // Pass 1: Draw dark outline (slightly longer for halo effect)
        let outline_end = hit.position + hit.normal * (indicator.length * 1.1);
        gizmos.line(start, outline_end, indicator.outline_color);

        // Pass 2: Draw grey core on top
        gizmos.line(start, end, indicator.color);
    }
}

/// Plugin for the normal indicator feature
pub struct NormalIndicatorPlugin;

impl Plugin for NormalIndicatorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NormalIndicatorState>().add_systems(
            Update,
            (
                update_normal_indicator,
                render_normal_indicator.after(update_normal_indicator),
            ),
        );
    }
}
