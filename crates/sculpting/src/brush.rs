//! Sculpt brush engine and dab generation.
//!
//! This module provides the brush system for sculpting, generating dabs
//! from input events and managing brush presets.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::types::{DeformationType, SculptDab, SculptStrokeHeader, SculptStrokePacket};

/// Falloff curve for brush influence.
///
/// Determines how brush strength decreases from center to edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum FalloffCurve {
    /// Linear falloff: strength = 1 - distance/radius
    #[default]
    Linear = 0,
    /// Smooth falloff: hermite interpolation
    Smooth = 1,
    /// Sharp falloff: quadratic decay
    Sharp = 2,
    /// Constant: full strength within radius
    Constant = 3,
    /// Sphere: spherical falloff (sqrt-based)
    Sphere = 4,
}

impl FalloffCurve {
    /// Calculate falloff strength at a given normalized distance (0.0 = center, 1.0 = edge).
    pub fn evaluate(&self, normalized_distance: f32) -> f32 {
        let d = normalized_distance.clamp(0.0, 1.0);
        match self {
            FalloffCurve::Linear => 1.0 - d,
            FalloffCurve::Smooth => {
                // Hermite smoothstep: 3d² - 2d³
                let t = 1.0 - d;
                t * t * (3.0 - 2.0 * t)
            }
            FalloffCurve::Sharp => {
                // Quadratic decay
                let t = 1.0 - d;
                t * t
            }
            FalloffCurve::Constant => 1.0,
            FalloffCurve::Sphere => {
                // Spherical: sqrt(1 - d²)
                (1.0 - d * d).max(0.0).sqrt()
            }
        }
    }
}

/// Brush preset configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrushPreset {
    /// Display name
    pub name: String,
    /// Deformation type
    pub deformation_type: DeformationType,
    /// Base radius in world units
    pub radius: f32,
    /// Strength multiplier (0.0 to 1.0)
    pub strength: f32,
    /// Falloff curve
    pub falloff: FalloffCurve,
    /// Whether to use pressure sensitivity for radius
    pub pressure_affects_radius: bool,
    /// Whether to use pressure sensitivity for strength
    pub pressure_affects_strength: bool,
    /// Spacing between dabs as fraction of radius (0.1 = 10% of radius)
    pub spacing: f32,
}

impl Default for BrushPreset {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            deformation_type: DeformationType::Push,
            radius: 0.25,
            strength: 0.5,
            falloff: FalloffCurve::Smooth,
            pressure_affects_radius: false,
            pressure_affects_strength: true,
            spacing: 0.25,
        }
    }
}

impl BrushPreset {
    /// Create a push brush preset.
    pub fn push() -> Self {
        Self {
            name: "Push".to_string(),
            deformation_type: DeformationType::Push,
            ..Default::default()
        }
    }

    /// Create a pull brush preset.
    pub fn pull() -> Self {
        Self {
            name: "Pull".to_string(),
            deformation_type: DeformationType::Pull,
            ..Default::default()
        }
    }

    /// Create a smooth brush preset.
    pub fn smooth() -> Self {
        Self {
            name: "Smooth".to_string(),
            deformation_type: DeformationType::Smooth,
            strength: 0.3,
            ..Default::default()
        }
    }

    /// Create a flatten brush preset.
    pub fn flatten() -> Self {
        Self {
            name: "Flatten".to_string(),
            deformation_type: DeformationType::Flatten,
            strength: 0.4,
            ..Default::default()
        }
    }

    /// Create an inflate brush preset.
    pub fn inflate() -> Self {
        Self {
            name: "Inflate".to_string(),
            deformation_type: DeformationType::Inflate,
            strength: 0.3,
            ..Default::default()
        }
    }

    /// Create a pinch brush preset.
    pub fn pinch() -> Self {
        Self {
            name: "Pinch".to_string(),
            deformation_type: DeformationType::Pinch,
            strength: 0.4,
            falloff: FalloffCurve::Sharp,
            ..Default::default()
        }
    }

    /// Create a grab brush preset.
    pub fn grab() -> Self {
        Self {
            name: "Grab".to_string(),
            deformation_type: DeformationType::Grab,
            strength: 1.0,
            falloff: FalloffCurve::Smooth,
            spacing: 0.0, // Continuous, no spacing
            ..Default::default()
        }
    }

    /// Create a crease brush preset.
    pub fn crease() -> Self {
        Self {
            name: "Crease".to_string(),
            deformation_type: DeformationType::Crease,
            strength: 0.5,
            falloff: FalloffCurve::Sharp,
            ..Default::default()
        }
    }

    /// Get effective radius based on pressure.
    pub fn effective_radius(&self, pressure: f32) -> f32 {
        if self.pressure_affects_radius {
            self.radius * (0.5 + 0.5 * pressure)
        } else {
            self.radius
        }
    }

    /// Get effective strength based on pressure.
    pub fn effective_strength(&self, pressure: f32) -> f32 {
        if self.pressure_affects_strength {
            self.strength * pressure
        } else {
            self.strength
        }
    }
}

/// Input event for brush stroke.
#[derive(Debug, Clone, Copy)]
pub struct BrushInput {
    /// World-space position of the brush
    pub position: Vec3,
    /// Surface normal at brush position (for orientation)
    pub normal: Vec3,
    /// Pressure (0.0 to 1.0)
    pub pressure: f32,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// State for an active stroke.
#[derive(Debug, Clone)]
pub struct StrokeState {
    /// Stroke identifier
    pub stroke_id: u64,
    /// Mesh being sculpted
    pub mesh_id: u32,
    /// Starting timestamp
    pub start_time_ms: u64,
    /// Last dab position (for spacing calculation)
    pub last_dab_position: Vec3,
    /// Distance accumulated since last dab (for spacing)
    pub distance_since_dab: f32,
    /// Base position for delta compression (current packet)
    pub base_position: Vec3,
    /// Current packet dabs
    pub current_dabs: Vec<SculptDab>,
    /// Completed packets
    pub completed_packets: Vec<SculptStrokePacket>,
}

impl StrokeState {
    /// Create a new stroke state.
    pub fn new(stroke_id: u64, mesh_id: u32, start_position: Vec3, timestamp_ms: u64) -> Self {
        Self {
            stroke_id,
            mesh_id,
            start_time_ms: timestamp_ms,
            last_dab_position: start_position,
            distance_since_dab: 0.0,
            base_position: start_position,
            current_dabs: Vec::new(),
            completed_packets: Vec::new(),
        }
    }
}

/// Sculpt brush engine for generating dabs from input.
#[derive(Debug)]
pub struct SculptBrushEngine {
    /// Current brush preset
    pub preset: BrushPreset,
    /// Active stroke state (None if not stroking)
    active_stroke: Option<StrokeState>,
    /// Next stroke ID
    next_stroke_id: u64,
    /// Delta scale factor for compression (positions × this = delta units)
    delta_scale: f32,
}

impl Default for SculptBrushEngine {
    fn default() -> Self {
        Self {
            preset: BrushPreset::default(),
            active_stroke: None,
            next_stroke_id: 0,
            delta_scale: 100.0, // 1 unit = 100 delta units
        }
    }
}

impl SculptBrushEngine {
    /// Create a new brush engine with the given preset.
    pub fn new(preset: BrushPreset) -> Self {
        Self {
            preset,
            ..Default::default()
        }
    }

    /// Check if a stroke is currently active.
    pub fn is_stroking(&self) -> bool {
        self.active_stroke.is_some()
    }

    /// Begin a new stroke.
    ///
    /// Returns the stroke ID.
    pub fn begin_stroke(&mut self, mesh_id: u32, input: BrushInput) -> u64 {
        let stroke_id = self.next_stroke_id;
        self.next_stroke_id += 1;

        self.active_stroke = Some(StrokeState::new(
            stroke_id,
            mesh_id,
            input.position,
            input.timestamp_ms,
        ));

        stroke_id
    }

    /// Update the stroke with new input.
    ///
    /// Returns dabs generated from this input (may be empty if spacing not met).
    pub fn update_stroke(&mut self, input: BrushInput) -> Vec<DabResult> {
        // Take the stroke out to avoid borrow conflicts
        let Some(mut stroke) = self.active_stroke.take() else {
            return Vec::new();
        };

        let mut results = Vec::new();
        let effective_radius = self.preset.effective_radius(input.pressure);
        let spacing_distance = effective_radius * self.preset.spacing;

        // For grab brush (spacing = 0), always emit a dab
        if spacing_distance <= 0.0 {
            let dab = self.create_dab(&mut stroke, input);
            results.push(dab);
            stroke.last_dab_position = input.position;
            self.active_stroke = Some(stroke);
            return results;
        }

        // Calculate distance from last dab
        let distance = input.position.distance(stroke.last_dab_position);
        stroke.distance_since_dab += distance;

        // Emit dabs along the path if spacing is exceeded
        if stroke.distance_since_dab >= spacing_distance {
            let direction = (input.position - stroke.last_dab_position).normalize_or_zero();
            let mut current_pos = stroke.last_dab_position;

            while stroke.distance_since_dab >= spacing_distance {
                current_pos += direction * spacing_distance;
                stroke.distance_since_dab -= spacing_distance;

                let dab_input = BrushInput {
                    position: current_pos,
                    normal: input.normal,
                    pressure: input.pressure,
                    timestamp_ms: input.timestamp_ms,
                };

                let dab = self.create_dab(&mut stroke, dab_input);
                results.push(dab);
            }

            stroke.last_dab_position = current_pos;
        }

        // Put the stroke back
        self.active_stroke = Some(stroke);
        results
    }

    /// End the current stroke and return the completed packets.
    pub fn end_stroke(&mut self) -> Option<Vec<SculptStrokePacket>> {
        let mut stroke = self.active_stroke.take()?;

        // Finalize current packet if it has dabs
        if !stroke.current_dabs.is_empty() {
            let packet = self.create_packet(&stroke);
            stroke.completed_packets.push(packet);
        }

        Some(stroke.completed_packets)
    }

    /// Cancel the current stroke without saving.
    pub fn cancel_stroke(&mut self) {
        self.active_stroke = None;
    }

    /// Create a dab from input, handling delta compression.
    fn create_dab(&mut self, stroke: &mut StrokeState, input: BrushInput) -> DabResult {
        // Calculate delta from base position
        let delta = input.position - stroke.base_position;
        let scaled_delta = delta * self.delta_scale;

        // Check if delta exceeds i8 range
        let needs_new_packet = scaled_delta.x.abs() > 127.0
            || scaled_delta.y.abs() > 127.0
            || scaled_delta.z.abs() > 127.0;

        if needs_new_packet && !stroke.current_dabs.is_empty() {
            // Finalize current packet
            let packet = self.create_packet(stroke);
            stroke.completed_packets.push(packet);
            stroke.current_dabs.clear();
            stroke.base_position = input.position;
        }

        // Create the dab
        let delta = input.position - stroke.base_position;
        let scaled_delta = delta * self.delta_scale;

        let dab = SculptDab {
            dx: (scaled_delta.x as i8).clamp(-127, 127),
            dy: (scaled_delta.y as i8).clamp(-127, 127),
            dz: (scaled_delta.z as i8).clamp(-127, 127),
            pressure: (input.pressure * 255.0) as u8,
            radius_scale: SculptDab::encode_radius_scale(
                self.preset.effective_radius(input.pressure) / self.preset.radius,
            ),
            normal_hint: SculptDab::encode_normal(input.normal),
            _padding: [0, 0],
        };

        stroke.current_dabs.push(dab);

        // Update base position for next dab (relative positioning)
        stroke.base_position = input.position;

        DabResult {
            position: input.position,
            normal: input.normal,
            radius: self.preset.effective_radius(input.pressure),
            strength: self.preset.effective_strength(input.pressure),
            dab,
        }
    }

    /// Create a packet from current stroke state.
    fn create_packet(&self, stroke: &StrokeState) -> SculptStrokePacket {
        // Use fixed-point for base position
        let base_scale = 1000.0;

        SculptStrokePacket {
            header: SculptStrokeHeader {
                version: 1,
                mesh_id: stroke.mesh_id,
                stroke_id: stroke.stroke_id,
                timestamp_ms: stroke.start_time_ms,
                deformation_type: self.preset.deformation_type,
                base_radius: (self.preset.radius * 1000.0) as u32,
                strength: (self.preset.strength * 255.0) as u8,
                flags: 0,
                base_x: (stroke.base_position.x * base_scale) as i32,
                base_y: (stroke.base_position.y * base_scale) as i32,
                base_z: (stroke.base_position.z * base_scale) as i32,
            },
            dabs: stroke.current_dabs.clone(),
        }
    }
}

/// Result of generating a dab, with decoded values for immediate use.
#[derive(Debug, Clone, Copy)]
pub struct DabResult {
    /// World-space position
    pub position: Vec3,
    /// Surface normal
    pub normal: Vec3,
    /// Effective radius
    pub radius: f32,
    /// Effective strength
    pub strength: f32,
    /// The compressed dab data
    pub dab: SculptDab,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_falloff_curves() {
        // All curves should be 1.0 at center
        assert!((FalloffCurve::Linear.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!((FalloffCurve::Smooth.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!((FalloffCurve::Sharp.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!((FalloffCurve::Constant.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!((FalloffCurve::Sphere.evaluate(0.0) - 1.0).abs() < 0.001);

        // All curves should be 0.0 at edge (except Constant)
        assert!((FalloffCurve::Linear.evaluate(1.0) - 0.0).abs() < 0.001);
        assert!((FalloffCurve::Smooth.evaluate(1.0) - 0.0).abs() < 0.001);
        assert!((FalloffCurve::Sharp.evaluate(1.0) - 0.0).abs() < 0.001);
        assert!((FalloffCurve::Constant.evaluate(1.0) - 1.0).abs() < 0.001);
        assert!((FalloffCurve::Sphere.evaluate(1.0) - 0.0).abs() < 0.001);

        // Smooth should have gradient = 0 at endpoints
        let smooth_near_start = FalloffCurve::Smooth.evaluate(0.01);
        let smooth_near_end = FalloffCurve::Smooth.evaluate(0.99);
        assert!(smooth_near_start > 0.99);
        assert!(smooth_near_end < 0.01);
    }

    #[test]
    fn test_brush_preset_defaults() {
        let preset = BrushPreset::default();
        assert_eq!(preset.deformation_type, DeformationType::Push);
        assert!((preset.radius - 0.25).abs() < 0.001);
        assert!((preset.strength - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_effective_radius_with_pressure() {
        let mut preset = BrushPreset::default();
        preset.radius = 1.0;
        preset.pressure_affects_radius = true;

        // At pressure 0.0, radius should be 0.5
        assert!((preset.effective_radius(0.0) - 0.5).abs() < 0.001);
        // At pressure 1.0, radius should be 1.0
        assert!((preset.effective_radius(1.0) - 1.0).abs() < 0.001);
        // At pressure 0.5, radius should be 0.75
        assert!((preset.effective_radius(0.5) - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_stroke_lifecycle() {
        let mut engine = SculptBrushEngine::default();
        assert!(!engine.is_stroking());

        let input = BrushInput {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            pressure: 1.0,
            timestamp_ms: 0,
        };

        let stroke_id = engine.begin_stroke(1, input);
        assert!(engine.is_stroking());
        assert_eq!(stroke_id, 0);

        // Update should generate dabs based on spacing
        let input2 = BrushInput {
            position: Vec3::new(1.0, 0.0, 0.0),
            normal: Vec3::Y,
            pressure: 1.0,
            timestamp_ms: 100,
        };
        let dabs = engine.update_stroke(input2);
        // With spacing 0.25 and radius 0.25, spacing_distance = 0.0625
        // Distance moved = 1.0, so we should get multiple dabs
        assert!(!dabs.is_empty());

        let packets = engine.end_stroke();
        assert!(packets.is_some());
        assert!(!engine.is_stroking());
    }

    #[test]
    fn test_grab_brush_continuous() {
        let mut engine = SculptBrushEngine::new(BrushPreset::grab());

        let input = BrushInput {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            pressure: 1.0,
            timestamp_ms: 0,
        };

        engine.begin_stroke(1, input);

        // Even small movement should generate a dab for grab brush
        let input2 = BrushInput {
            position: Vec3::new(0.01, 0.0, 0.0),
            normal: Vec3::Y,
            pressure: 1.0,
            timestamp_ms: 10,
        };
        let dabs = engine.update_stroke(input2);
        assert_eq!(dabs.len(), 1);

        engine.end_stroke();
    }
}
