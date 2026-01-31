//! Configurable sun/sky lighting system
//!
//! Supports time-of-day simulation where sun position is calculated
//! based on time (0-24 hours). Cloudiness affects ambient light color
//! and sun intensity.
//!
//! With the `atmosphere` feature enabled, uses Bevy's built-in atmospheric
//! scattering for realistic sky rendering.

#[cfg(not(feature = "atmosphere"))]
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use pentimento_ipc::LightingSettings;

#[cfg(feature = "atmosphere")]
use bevy::prelude::light_consts::lux;
#[cfg(feature = "atmosphere")]
use bevy::pbr::ScatteringMedium;

/// Calculate sun direction from time of day and azimuth angle
///
/// Time is in hours (0.0-24.0). Sunrise is at 6:00, sunset at 18:00.
/// Azimuth angle rotates the sun's path around Y-axis (0=east, 90=south, etc.)
/// Returns a normalized direction vector pointing toward the sun.
#[cfg(not(feature = "atmosphere"))]
fn calculate_sun_direction_from_time(time_of_day: f32, azimuth_angle: f32) -> Vec3 {
    // Normalize time to 0-1 range within daylight hours (6:00-18:00)
    // Before 6:00 or after 18:00, sun is below horizon
    let clamped_time = time_of_day.clamp(6.0, 18.0);
    let normalized_time = (clamped_time - 6.0) / 12.0; // 0.0 at sunrise, 1.0 at sunset

    // Sun arc: rises in east (negative X), sets in west (positive X)
    // Y is height, peaks at noon (normalized_time = 0.5)
    let sun_angle = normalized_time * std::f32::consts::PI;

    // Base direction without azimuth rotation
    let base_dir = Vec3::new(
        -sun_angle.cos(), // East to west
        sun_angle.sin().max(0.05), // Height (keep slightly above horizon for lighting)
        -0.3, // Slight southern offset (typical for northern hemisphere)
    );

    // Rotate by azimuth angle around Y axis
    let azimuth_rad = azimuth_angle.to_radians();
    let rotation = Quat::from_rotation_y(azimuth_rad);

    (rotation * base_dir).normalize()
}

/// Calculate sun color based on time of day (warmer at sunrise/sunset)
#[cfg(not(feature = "atmosphere"))]
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
#[cfg(not(feature = "atmosphere"))]
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
#[cfg(not(feature = "atmosphere"))]
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

/// Calculate moon light intensity based on moon phase and time of day
///
/// Moon only provides light at night (18:00 - 6:00).
/// Intensity scales from ~0.001 lux (new moon) to ~0.1 lux (full moon).
#[cfg(not(feature = "atmosphere"))]
fn calculate_moon_intensity(moon_phase: f32, time_of_day: f32, cloudiness: f32) -> f32 {
    // Only provide light when sun is below horizon (night time)
    let is_night = time_of_day < 6.0 || time_of_day > 18.0;

    if !is_night {
        return 0.0;
    }

    // Moon intensity based on phase
    // Full moon (~0.1 lux), new moon (~0.001 lux starlight)
    let base_intensity = 0.001 + (moon_phase * 0.099);

    // Moon height varies through the night (peaks around midnight)
    let night_progress = if time_of_day > 18.0 {
        (time_of_day - 18.0) / 6.0 // 18:00 -> 0.0, 24:00 -> 1.0
    } else {
        1.0 - (time_of_day / 6.0) // 0:00 -> 1.0, 6:00 -> 0.0
    };
    let moon_height = (night_progress * std::f32::consts::PI).sin().max(0.1);

    // Clouds also affect moonlight
    let cloud_factor = 1.0 - (cloudiness * 0.9);

    base_intensity * moon_height * cloud_factor
}

/// Apply pollution effect to sun color (shifts toward yellow/brown)
#[cfg(not(feature = "atmosphere"))]
fn apply_pollution_to_color(base_color: [f32; 3], pollution: f32) -> [f32; 3] {
    // Pollution shifts color toward yellow/brown and reduces saturation
    let pollution_tint = [1.0, 0.85, 0.6]; // Yellowish-brown
    let blend = pollution * 0.4;

    [
        base_color[0] * (1.0 - blend) + pollution_tint[0] * blend,
        base_color[1] * (1.0 - blend) + pollution_tint[1] * blend,
        base_color[2] * (1.0 - blend) + pollution_tint[2] * blend,
    ]
}

/// Apply pollution effect to light intensity (reduces by up to 50%)
#[cfg(not(feature = "atmosphere"))]
fn apply_pollution_to_intensity(base_intensity: f32, pollution: f32) -> f32 {
    // Pollution reduces direct sunlight (up to 50% reduction at max pollution)
    base_intensity * (1.0 - pollution * 0.5)
}

/// Apply pollution effect to ambient light (makes it grayer and slightly brighter)
#[cfg(not(feature = "atmosphere"))]
fn apply_pollution_to_ambient(base_ambient: [f32; 3], pollution: f32) -> [f32; 3] {
    // Pollution makes ambient more gray/brown (scattered light from particles)
    let hazy_gray = [0.65, 0.6, 0.55];
    let blend = pollution * 0.5;

    [
        base_ambient[0] * (1.0 - blend) + hazy_gray[0] * blend,
        base_ambient[1] * (1.0 - blend) + hazy_gray[1] * blend,
        base_ambient[2] * (1.0 - blend) + hazy_gray[2] * blend,
    ]
}

/// Marker component for the sun directional light
#[derive(Component)]
pub struct SunLight;

/// Resource for current lighting settings
///
/// Uses Bevy's built-in change detection - the `update_lighting` system
/// checks `is_changed()` instead of a manual dirty flag.
#[derive(Resource)]
pub struct SceneLighting {
    /// Current lighting configuration
    pub settings: LightingSettings,
}

impl Default for SceneLighting {
    fn default() -> Self {
        Self {
            settings: LightingSettings::default(),
        }
    }
}

/// Resource tracking atmosphere rendering state (atmosphere feature only)
#[cfg(feature = "atmosphere")]
#[derive(Resource)]
pub struct AtmosphereState {
    /// Handle to the scattering medium asset
    pub medium: Handle<ScatteringMedium>,
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
#[cfg(feature = "atmosphere")]
fn setup_lighting(
    mut commands: Commands,
    lighting: Res<SceneLighting>,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
) {
    let settings = &lighting.settings;

    // Calculate initial sun rotation from time of day
    // Sun rotates around X-axis: midnight=0, noon=PI
    let sun_angle = (settings.time_of_day / 24.0) * std::f32::consts::TAU;

    // Spawn directional light (sun) with raw sunlight - atmosphere will attenuate it
    commands.spawn((
        DirectionalLight {
            illuminance: lux::RAW_SUNLIGHT,
            color: Color::WHITE, // Atmosphere handles color tinting
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(sun_angle)),
        SunLight,
    ));

    // Create and store the scattering medium for atmosphere
    let medium = scattering_mediums.add(ScatteringMedium::default());
    commands.insert_resource(AtmosphereState { medium });

    // No GlobalAmbientLight - atmosphere IBL handles ambient lighting
    info!("Scene lighting initialized with atmosphere");
}

/// Spawn the sun light and ambient light (non-atmosphere version)
#[cfg(not(feature = "atmosphere"))]
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

/// Update lighting when settings change (atmosphere version)
///
/// Uses Bevy's change detection via `is_changed()` instead of a manual dirty flag.
#[cfg(feature = "atmosphere")]
fn update_lighting(
    lighting: Res<SceneLighting>,
    mut sun_query: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
) {
    if !lighting.is_changed() {
        return;
    }

    let use_time_of_day = lighting.settings.use_time_of_day;
    let time_of_day = lighting.settings.time_of_day;
    let cloudiness = lighting.settings.cloudiness;
    let azimuth_angle = lighting.settings.azimuth_angle;
    let pollution = lighting.settings.pollution;

    for (mut light, mut transform) in sun_query.iter_mut() {
        if use_time_of_day {
            // Convert time_of_day (0-24h) to sun rotation angle
            // Sun rotates around X-axis: midnight=0, noon=PI
            let sun_angle = (time_of_day / 24.0) * std::f32::consts::TAU;

            // Combine rotations: sun arc (around X) + azimuth (around Y)
            let azimuth_rad = azimuth_angle.to_radians();
            let rotation = Quat::from_rotation_y(azimuth_rad) * Quat::from_rotation_x(sun_angle);
            *transform = Transform::from_rotation(rotation);
        } else {
            // Use explicit direction from settings (still apply azimuth rotation)
            let base_dir = Vec3::from_array(lighting.settings.sun_direction).normalize();
            let azimuth_rad = azimuth_angle.to_radians();
            let rotation = Quat::from_rotation_y(azimuth_rad);
            let direction = (rotation * base_dir).normalize();
            *transform = Transform::default().looking_to(-direction, Vec3::Y);
        }

        // With atmosphere, use raw sunlight - atmosphere handles attenuation
        // Cloudiness and pollution modulate illuminance
        let cloud_factor = 1.0 - (cloudiness * 0.3); // Up to 30% reduction for thick clouds
        let pollution_factor = 1.0 - (pollution * 0.4); // Up to 40% reduction for heavy pollution
        light.illuminance = lux::RAW_SUNLIGHT * cloud_factor * pollution_factor;
    }

    info!(
        "Scene lighting updated (atmosphere): time={:.1}h, cloudiness={:.0}%, azimuth={:.0}°, pollution={:.0}%",
        time_of_day,
        cloudiness * 100.0,
        azimuth_angle,
        pollution * 100.0
    );
}

/// Update lighting when settings change (non-atmosphere version)
///
/// Uses Bevy's change detection via `is_changed()` instead of a manual dirty flag.
#[cfg(not(feature = "atmosphere"))]
fn update_lighting(
    lighting: Res<SceneLighting>,
    mut sun_query: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
) {
    if !lighting.is_changed() {
        return;
    }

    // Copy settings to avoid borrow issues
    let use_time_of_day = lighting.settings.use_time_of_day;
    let time_of_day = lighting.settings.time_of_day;
    let cloudiness = lighting.settings.cloudiness;
    let base_sun_intensity = lighting.settings.sun_intensity;
    let base_ambient_intensity = lighting.settings.ambient_intensity;
    let moon_phase = lighting.settings.moon_phase;
    let azimuth_angle = lighting.settings.azimuth_angle;
    let pollution = lighting.settings.pollution;

    // Determine sun direction, color, and intensity
    let (sun_direction, sun_color, sun_intensity) = if use_time_of_day {
        // Calculate from time of day, cloudiness, and azimuth
        let direction = calculate_sun_direction_from_time(time_of_day, azimuth_angle);
        let color = calculate_sun_color_from_time(time_of_day);
        let intensity = calculate_sun_intensity(time_of_day, cloudiness, base_sun_intensity);
        (direction, color, intensity)
    } else {
        // Use explicit values from settings (still apply azimuth rotation)
        let base_dir = Vec3::from_array(lighting.settings.sun_direction).normalize();
        let azimuth_rad = azimuth_angle.to_radians();
        let rotation = Quat::from_rotation_y(azimuth_rad);
        let direction = (rotation * base_dir).normalize();
        (
            direction,
            lighting.settings.sun_color,
            lighting.settings.sun_intensity,
        )
    };

    // Apply pollution effects to sun
    let sun_color = apply_pollution_to_color(sun_color, pollution);
    let sun_intensity = apply_pollution_to_intensity(sun_intensity, pollution);

    // Determine ambient color and intensity
    let (ambient_color, ambient_intensity) = if use_time_of_day {
        let color = calculate_ambient_color(time_of_day, cloudiness);
        // Cloudiness increases ambient (more scattered light)
        let intensity = base_ambient_intensity * (1.0 + cloudiness * 0.5);
        (color, intensity)
    } else {
        (lighting.settings.ambient_color, lighting.settings.ambient_intensity)
    };

    // Apply pollution effects to ambient
    let ambient_color = apply_pollution_to_ambient(ambient_color, pollution);
    // Pollution slightly increases ambient (more scattered light from particles)
    let ambient_intensity = ambient_intensity * (1.0 + pollution * 0.3);

    // Add moon contribution to ambient at night
    let moon_intensity = calculate_moon_intensity(moon_phase, time_of_day, cloudiness);
    let total_ambient_intensity = ambient_intensity + moon_intensity * 1000.0; // Scale moon lux to match ambient units

    // Update sun light
    for (mut light, mut transform) in sun_query.iter_mut() {
        light.illuminance = sun_intensity;
        light.color = Color::srgb(sun_color[0], sun_color[1], sun_color[2]);
        *transform = Transform::default().looking_to(-sun_direction, Vec3::Y);
    }

    // Update global ambient light
    ambient_light.color = Color::srgb(ambient_color[0], ambient_color[1], ambient_color[2]);
    ambient_light.brightness = total_ambient_intensity;

    info!(
        "Scene lighting updated: time={:.1}h, cloudiness={:.0}%, moon={:.0}%, azimuth={:.0}°, pollution={:.0}%",
        time_of_day,
        cloudiness * 100.0,
        moon_phase * 100.0,
        azimuth_angle,
        pollution * 100.0
    );
}
