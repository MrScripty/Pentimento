//! Configurable sun/sky lighting system

use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use pentimento_ipc::LightingSettings;

/// Marker component for the sun directional light
#[derive(Component)]
pub struct SunLight;

/// Resource for current lighting settings
#[derive(Resource)]
pub struct SceneLighting {
    /// Current lighting configuration
    pub settings: LightingSettings,
    /// Flag indicating settings have changed and need to be applied
    pub dirty: bool,
}

impl Default for SceneLighting {
    fn default() -> Self {
        Self {
            settings: LightingSettings::default(),
            dirty: true, // Apply on first frame
        }
    }
}

impl SceneLighting {
    /// Update settings and mark as dirty
    pub fn update(&mut self, settings: LightingSettings) {
        self.settings = settings;
        self.dirty = true;
    }
}

/// Plugin for configurable scene lighting
pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneLighting>()
            .add_systems(Startup, setup_lighting)
            .add_systems(Update, update_lighting);
    }
}

/// Spawn the sun light and ambient light
fn setup_lighting(mut commands: Commands, lighting: Res<SceneLighting>) {
    let settings = &lighting.settings;

    // Calculate direction from the settings (the setting stores the "to light" direction,
    // but Transform::looking_to needs the "forward" direction, which is -direction)
    let direction = Vec3::from_array(settings.sun_direction).normalize();

    // Spawn directional light (sun)
    commands.spawn((
        DirectionalLight {
            illuminance: settings.sun_intensity,
            color: Color::srgb(
                settings.sun_color[0],
                settings.sun_color[1],
                settings.sun_color[2],
            ),
            shadows_enabled: true,
            ..default()
        },
        // looking_to takes the forward direction; sun shines in -direction
        Transform::default().looking_to(-direction, Vec3::Y),
        SunLight,
    ));

    // Set global ambient light (it's a resource, not an entity)
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(
            settings.ambient_color[0],
            settings.ambient_color[1],
            settings.ambient_color[2],
        ),
        brightness: settings.ambient_intensity,
        ..default()
    });

    info!("Scene lighting initialized");
}

/// Update lighting when settings change
fn update_lighting(
    mut lighting: ResMut<SceneLighting>,
    mut sun_query: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
) {
    if !lighting.dirty {
        return;
    }

    let settings = &lighting.settings;

    // Update sun light
    for (mut light, mut transform) in sun_query.iter_mut() {
        let direction = Vec3::from_array(settings.sun_direction).normalize();

        light.illuminance = settings.sun_intensity;
        light.color = Color::srgb(
            settings.sun_color[0],
            settings.sun_color[1],
            settings.sun_color[2],
        );

        *transform = Transform::default().looking_to(-direction, Vec3::Y);
    }

    // Update global ambient light
    ambient_light.color = Color::srgb(
        settings.ambient_color[0],
        settings.ambient_color[1],
        settings.ambient_color[2],
    );
    ambient_light.brightness = settings.ambient_intensity;

    lighting.dirty = false;
    info!("Scene lighting updated");
}
