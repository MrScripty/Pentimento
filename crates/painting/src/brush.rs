//! Brush engine for dab generation
//!
//! This module provides a simple brush system that interpolates input
//! points and generates dabs for painting. This is a placeholder for
//! future libmypaint FFI integration.

use tracing::debug;

/// Brush preset configuration
#[derive(Debug, Clone)]
pub struct BrushPreset {
    /// Unique preset ID
    pub id: u32,
    /// Human-readable name
    pub name: String,
    /// Base diameter in pixels
    pub base_size: f32,
    /// Size at pressure 0
    pub min_size: f32,
    /// Size at pressure 1
    pub max_size: f32,
    /// Hardness: 0.0 = soft, 1.0 = hard
    pub hardness: f32,
    /// Base opacity 0.0-1.0
    pub opacity: f32,
    /// Spacing as fraction of size (e.g., 0.25 = 25% of diameter)
    pub spacing: f32,
}

impl Default for BrushPreset {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Default".to_string(),
            base_size: 20.0,
            min_size: 5.0,
            max_size: 50.0,
            hardness: 0.8,
            opacity: 1.0,
            spacing: 0.25,
        }
    }
}

impl BrushPreset {
    /// Create a new brush preset with the given parameters
    pub fn new(
        id: u32,
        name: impl Into<String>,
        base_size: f32,
        min_size: f32,
        max_size: f32,
        hardness: f32,
        opacity: f32,
        spacing: f32,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            base_size,
            min_size,
            max_size,
            hardness: hardness.clamp(0.0, 1.0),
            opacity: opacity.clamp(0.0, 1.0),
            spacing: spacing.max(0.01), // Prevent zero spacing
        }
    }

    /// Calculate brush size based on pressure
    pub fn size_for_pressure(&self, pressure: f32) -> f32 {
        let pressure = pressure.clamp(0.0, 1.0);
        self.min_size + (self.max_size - self.min_size) * pressure
    }
}

/// Output from brush engine for a single dab
#[derive(Debug, Clone)]
pub struct DabOutput {
    /// X position in surface coordinates
    pub x: f32,
    /// Y position in surface coordinates
    pub y: f32,
    /// Diameter in pixels
    pub size: f32,
    /// Hardness 0.0-1.0
    pub hardness: f32,
    /// Opacity 0.0-1.0
    pub opacity: f32,
}

/// Brush engine that generates dabs from input
///
/// The brush engine interpolates between input points based on the
/// spacing setting and calculates size from pressure.
pub struct BrushEngine {
    /// Current brush preset
    preset: BrushPreset,
    /// Last position (None if stroke not started)
    last_pos: Option<(f32, f32)>,
    /// Last pressure for interpolation
    last_pressure: f32,
    /// Accumulated distance since last dab
    distance_accumulator: f32,
}

impl BrushEngine {
    /// Create a new brush engine with the given preset
    pub fn new(preset: BrushPreset) -> Self {
        Self {
            preset,
            last_pos: None,
            last_pressure: 0.0,
            distance_accumulator: 0.0,
        }
    }

    /// Create a brush engine with default preset
    pub fn with_default_preset() -> Self {
        Self::new(BrushPreset::default())
    }

    /// Get the current preset
    pub fn preset(&self) -> &BrushPreset {
        &self.preset
    }

    /// Set a new brush preset
    pub fn set_preset(&mut self, preset: BrushPreset) {
        self.preset = preset;
    }

    /// Start a new stroke
    pub fn begin_stroke(&mut self) {
        self.last_pos = None;
        self.last_pressure = 0.0;
        self.distance_accumulator = 0.0;
    }

    /// Process input and generate dabs
    ///
    /// Returns a list of dabs to apply. The brush engine interpolates
    /// between the last position and the new position, placing dabs
    /// according to the spacing setting.
    pub fn stroke_to(&mut self, x: f32, y: f32, pressure: f32) -> Vec<DabOutput> {
        let pressure = pressure.clamp(0.0, 1.0);
        let mut dabs = Vec::new();

        // First point in stroke - generate initial dab
        let Some((last_x, last_y)) = self.last_pos else {
            self.last_pos = Some((x, y));
            self.last_pressure = pressure;
            self.distance_accumulator = 0.0;

            // Generate first dab at starting position
            let size = self.preset.size_for_pressure(pressure);
            debug!(
                "BrushEngine::stroke_to: FIRST dab at ({:.1}, {:.1}), size={:.1}",
                x, y, size
            );
            dabs.push(DabOutput {
                x,
                y,
                size,
                hardness: self.preset.hardness,
                opacity: self.preset.opacity,
            });

            return dabs;
        };

        // Calculate distance from last point
        let dx = x - last_x;
        let dy = y - last_y;
        let distance = (dx * dx + dy * dy).sqrt();

        if distance < 0.001 {
            // No significant movement
            return dabs;
        }

        // Direction vector (reserved for future use with angle calculations)
        let _dir_x = dx / distance;
        let _dir_y = dy / distance;

        // Add distance to accumulator
        self.distance_accumulator += distance;

        // Calculate average size for spacing calculation
        let avg_pressure = (self.last_pressure + pressure) / 2.0;
        let avg_size = self.preset.size_for_pressure(avg_pressure);
        let spacing_distance = avg_size * self.preset.spacing;

        if spacing_distance < 0.001 {
            // Prevent infinite loop with zero spacing
            self.last_pos = Some((x, y));
            self.last_pressure = pressure;
            return dabs;
        }

        // Generate dabs along the path
        let mut current_distance = 0.0;
        let mut dab_start = spacing_distance - (self.distance_accumulator - distance);

        // If we have accumulated enough distance for a dab
        if dab_start < 0.0 {
            dab_start = 0.0;
        }

        while dab_start <= distance {
            // Interpolation factor along the segment
            let t = dab_start / distance;

            // Interpolate position
            let dab_x = last_x + dx * t;
            let dab_y = last_y + dy * t;

            // Interpolate pressure
            let dab_pressure = self.last_pressure + (pressure - self.last_pressure) * t;
            let size = self.preset.size_for_pressure(dab_pressure);

            dabs.push(DabOutput {
                x: dab_x,
                y: dab_y,
                size,
                hardness: self.preset.hardness,
                opacity: self.preset.opacity,
            });

            current_distance = dab_start;
            dab_start += spacing_distance;
        }

        // Update distance accumulator for next segment
        self.distance_accumulator = distance - current_distance;
        if self.distance_accumulator < 0.0 {
            self.distance_accumulator = 0.0;
        }

        // Update state for next call
        self.last_pos = Some((x, y));
        self.last_pressure = pressure;

        if !dabs.is_empty() {
            debug!(
                "BrushEngine::stroke_to: generated {} dabs along path from ({:.1}, {:.1}) to ({:.1}, {:.1})",
                dabs.len(), last_x, last_y, x, y
            );
        }

        dabs
    }

    /// End the current stroke
    pub fn end_stroke(&mut self) {
        self.last_pos = None;
        self.last_pressure = 0.0;
        self.distance_accumulator = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_preset_default() {
        let preset = BrushPreset::default();
        assert_eq!(preset.id, 0);
        assert_eq!(preset.name, "Default");
        assert_eq!(preset.base_size, 20.0);
        assert!(preset.spacing > 0.0);
    }

    #[test]
    fn test_brush_preset_size_for_pressure() {
        let preset = BrushPreset {
            min_size: 10.0,
            max_size: 50.0,
            ..Default::default()
        };

        assert!((preset.size_for_pressure(0.0) - 10.0).abs() < 0.001);
        assert!((preset.size_for_pressure(1.0) - 50.0).abs() < 0.001);
        assert!((preset.size_for_pressure(0.5) - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_brush_engine_first_dab() {
        let mut engine = BrushEngine::with_default_preset();
        engine.begin_stroke();

        let dabs = engine.stroke_to(100.0, 100.0, 1.0);

        assert_eq!(dabs.len(), 1);
        assert!((dabs[0].x - 100.0).abs() < 0.001);
        assert!((dabs[0].y - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_brush_engine_interpolation() {
        let preset = BrushPreset {
            base_size: 20.0,
            min_size: 20.0,
            max_size: 20.0,
            spacing: 0.5, // 50% of size = 10 pixels
            ..Default::default()
        };
        let mut engine = BrushEngine::new(preset);
        engine.begin_stroke();

        // First dab at start
        let dabs = engine.stroke_to(0.0, 0.0, 1.0);
        assert_eq!(dabs.len(), 1);

        // Move 50 pixels - should generate ~5 dabs (50 / 10 = 5)
        let dabs = engine.stroke_to(50.0, 0.0, 1.0);
        assert!(dabs.len() >= 4 && dabs.len() <= 6);
    }

    #[test]
    fn test_brush_engine_no_dabs_for_small_movement() {
        let preset = BrushPreset {
            base_size: 20.0,
            min_size: 20.0,
            max_size: 20.0,
            spacing: 0.5, // 10 pixels
            ..Default::default()
        };
        let mut engine = BrushEngine::new(preset);
        engine.begin_stroke();

        // First dab
        engine.stroke_to(0.0, 0.0, 1.0);

        // Move less than spacing distance - should generate no new dabs
        let dabs = engine.stroke_to(5.0, 0.0, 1.0);
        assert_eq!(dabs.len(), 0);
    }

    #[test]
    fn test_brush_engine_end_stroke() {
        let mut engine = BrushEngine::with_default_preset();
        engine.begin_stroke();
        engine.stroke_to(0.0, 0.0, 1.0);
        engine.stroke_to(50.0, 0.0, 1.0);
        engine.end_stroke();

        // After ending, next stroke_to should generate first dab again
        engine.begin_stroke();
        let dabs = engine.stroke_to(100.0, 100.0, 1.0);
        assert_eq!(dabs.len(), 1);
    }
}
