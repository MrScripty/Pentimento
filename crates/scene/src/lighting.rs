//! Configurable sun/sky lighting system
//!
//! Supports time-of-day simulation where sun position is calculated
//! based on time (0-24 hours). Cloudiness affects ambient light color
//! and sun intensity.

use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use pentimento_ipc::LightingSettings;

/// Calculate sun direction from time of day
///
/// Time is in hours (0.0-24.0). Sunrise is at 6:00, sunset at 18:00.
/// Returns a normalized direction vector pointing toward the sun.
fn calculate_sun_direction_from_time(time_of_day: f32) -> Vec3 {
    // Normalize time to 0-1 range within daylight hours (6:00-18:00)
    // Before 6:00 or after 18:00, sun is below horizon
    let clamped_time = time_of_day.clamp(6.0, 18.0);
    let normalized_time = (clamped_time - 6.0) / 12.0; // 0.0 at sunrise, 1.0 at sunset

    // Sun arc: rises in east (negative X), sets in west (positive X)
    // Y is height, peaks at noon (normalized_time = 0.5)
    let sun_angle = normalized_time * std::f32::consts::PI;

    Vec3::new(
        -sun_angle.cos(), // East to west
        sun_angle.sin().max(0.05), // Height (keep slightly above horizon for lighting)
        -0.3, // Slight southern offset (typical for northern hemisphere)
    )
    .normalize()
}

/// Calculate sun color based on time of day (warmer at sunrise/sunset)
fn calculate_sun_color_from_time(time_of_day: f32) -> [f32; 3] {
    let clamped_time = time_of_day.clamp(6.0, 18.0);
    let normalized_time = (clamped_time - 6.0) / 12.0;
    let sun_height = (normalized_time * std::f32::consts::PI).sin();

    // Low sun = warm orange, high sun = white
    if sun_height < 0.3 {
        // Sunrise/sunset: warm orange
        let warmth = 1.0 - (sun_height / 0.3);
        [
            1.0,
            0.98 - warmth * 0.3, // More orange
            0.95 - warmth * 0.45, // Less blue
        ]
    } else {
        // Midday: slightly warm white
        [1.0, 0.98, 0.95]
    }
}

/// Calculate ambient color based on time and cloudiness
fn calculate_ambient_color(time_of_day: f32, cloudiness: f32) -> [f32; 3] {
    let clamped_time = time_of_day.clamp(6.0, 18.0);
    let normalized_time = (clamped_time - 6.0) / 12.0;
    let sun_height = (normalized_time * std::f32::consts::PI).sin();

    // Base ambient color varies with time of day
    let base_color = if sun_height < 0.3 {
        // Sunrise/sunset: warm amber ambient
        [0.8, 0.6, 0.4]
    } else {
        // Midday: sky blue ambient
        [0.6, 0.7, 1.0]
    };

    // Cloudiness shifts toward gray
    let gray = [0.7, 0.7, 0.7];
    [
        base_color[0] + (gray[0] - base_color[0]) * cloudiness,
        base_color[1] + (gray[1] - base_color[1]) * cloudiness,
        base_color[2] + (gray[2] - base_color[2]) * cloudiness,
    ]
}

/// Calculate sun intensity based on time and cloudiness
fn calculate_sun_intensity(time_of_day: f32, cloudiness: f32, base_intensity: f32) -> f32 {
    let clamped_time = time_of_day.clamp(6.0, 18.0);
    let normalized_time = (clamped_time - 6.0) / 12.0;
    let sun_height = (normalized_time * std::f32::consts::PI).sin();

    // Intensity based on sun height
    let height_factor = sun_height.max(0.1);

    // Cloudiness reduces intensity (clouds block light)
    let cloud_factor = 1.0 - (cloudiness * 0.8); // Max 80% reduction

    base_intensity * height_factor * cloud_factor
}

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

    // Copy settings to avoid borrow issues
    let use_time_of_day = lighting.settings.use_time_of_day;
    let time_of_day = lighting.settings.time_of_day;
    let cloudiness = lighting.settings.cloudiness;
    let base_sun_intensity = lighting.settings.sun_intensity;
    let base_ambient_intensity = lighting.settings.ambient_intensity;

    // Determine sun direction, color, and intensity
    let (sun_direction, sun_color, sun_intensity) = if use_time_of_day {
        // Calculate from time of day and cloudiness
        let direction = calculate_sun_direction_from_time(time_of_day);
        let color = calculate_sun_color_from_time(time_of_day);
        let intensity = calculate_sun_intensity(time_of_day, cloudiness, base_sun_intensity);
        (direction, color, intensity)
    } else {
        // Use explicit values from settings
        (
            Vec3::from_array(lighting.settings.sun_direction).normalize(),
            lighting.settings.sun_color,
            lighting.settings.sun_intensity,
        )
    };

    // Determine ambient color and intensity
    let (ambient_color, ambient_intensity) = if use_time_of_day {
        let color = calculate_ambient_color(time_of_day, cloudiness);
        // Cloudiness increases ambient (more scattered light)
        let intensity = base_ambient_intensity * (1.0 + cloudiness * 0.5);
        (color, intensity)
    } else {
        (lighting.settings.ambient_color, lighting.settings.ambient_intensity)
    };

    // Update sun light
    for (mut light, mut transform) in sun_query.iter_mut() {
        light.illuminance = sun_intensity;
        light.color = Color::srgb(sun_color[0], sun_color[1], sun_color[2]);
        *transform = Transform::default().looking_to(-sun_direction, Vec3::Y);
    }

    // Update global ambient light
    ambient_light.color = Color::srgb(ambient_color[0], ambient_color[1], ambient_color[2]);
    ambient_light.brightness = ambient_intensity;

    lighting.dirty = false;
    info!(
        "Scene lighting updated: time={:.1}h, cloudiness={:.0}%",
        time_of_day,
        cloudiness * 100.0
    );
}
