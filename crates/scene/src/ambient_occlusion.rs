//! Screen-space ambient occlusion (SSAO) control system
//!
//! Provides configurable SSAO settings that can be updated via IPC.
//! Note: SSAO is not supported on WebGL2/WASM targets.

use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::prelude::*;
use pentimento_ipc::AmbientOcclusionSettings;

use crate::camera::MainCamera;

/// Resource for current ambient occlusion settings
#[derive(Resource)]
pub struct SceneAmbientOcclusion {
    /// Current AO configuration
    pub settings: AmbientOcclusionSettings,
    /// Flag indicating settings have changed and need to be applied
    pub dirty: bool,
}

impl Default for SceneAmbientOcclusion {
    fn default() -> Self {
        Self {
            settings: AmbientOcclusionSettings::default(),
            dirty: false, // Start disabled, don't apply until requested
        }
    }
}

impl SceneAmbientOcclusion {
    /// Update settings and mark as dirty
    pub fn update(&mut self, settings: AmbientOcclusionSettings) {
        self.settings = settings;
        self.dirty = true;
    }
}

/// Plugin for configurable screen-space ambient occlusion
pub struct AmbientOcclusionPlugin;

impl Plugin for AmbientOcclusionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneAmbientOcclusion>()
            .add_systems(Update, update_ambient_occlusion);
    }
}

/// Update SSAO settings when changed
fn update_ambient_occlusion(
    mut commands: Commands,
    mut ao_resource: ResMut<SceneAmbientOcclusion>,
    camera_query: Query<Entity, With<MainCamera>>,
    ssao_query: Query<Entity, (With<MainCamera>, With<ScreenSpaceAmbientOcclusion>)>,
) {
    if !ao_resource.dirty {
        return;
    }
    ao_resource.dirty = false;

    let settings = &ao_resource.settings;

    for camera_entity in camera_query.iter() {
        if settings.enabled {
            // Convert quality level to Bevy's enum
            let quality = match settings.quality_level {
                0 => bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Low,
                1 => bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Medium,
                2 => bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::High,
                _ => bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel::Ultra,
            };

            // Insert or update SSAO component
            commands.entity(camera_entity).insert(ScreenSpaceAmbientOcclusion {
                quality_level: quality,
                constant_object_thickness: settings.constant_object_thickness,
            });

            info!(
                "SSAO enabled: quality={:?}, thickness={}",
                quality, settings.constant_object_thickness
            );
        } else {
            // Remove SSAO component if it exists
            if ssao_query.contains(camera_entity) {
                commands
                    .entity(camera_entity)
                    .remove::<ScreenSpaceAmbientOcclusion>();
                info!("SSAO disabled");
            }
        }
    }
}
