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

    /// Calculate falloff strength with hardness.
    ///
    /// Hardness (0.0–1.0) defines an inner zone of full strength.
    /// Within `radius * hardness`, strength is 1.0. Beyond that, the falloff
    /// curve applies over the remaining distance to the edge.
    pub fn evaluate_with_hardness(&self, normalized_distance: f32, hardness: f32) -> f32 {
        let d = normalized_distance.clamp(0.0, 1.0);
        let h = hardness.clamp(0.0, 1.0);
        if d <= h {
            return 1.0;
        }
        if h >= 1.0 {
            return 1.0;
        }
        // Remap (h..1.0) to (0.0..1.0) for curve evaluation
        let remapped = (d - h) / (1.0 - h);
        self.evaluate(remapped)
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
    /// Auto-smooth strength applied after each dab (0.0 = off, 1.0 = full).
    /// Dampens high-frequency surface ripples from dab boundaries.
    #[serde(default = "default_autosmooth")]
    pub autosmooth: f32,
    /// Hardness (0.0 to 1.0). Defines the inner zone of full strength as a
    /// fraction of the radius. The falloff curve only applies beyond this zone.
    #[serde(default = "default_hardness")]
    pub hardness: f32,
}

fn default_autosmooth() -> f32 {
    0.5
}

fn default_hardness() -> f32 {
    0.5
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
            autosmooth: 0.5,
            hardness: 0.5,
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
            autosmooth: 0.0,
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
            autosmooth: 0.0,
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
    /// Last emitted dab position (independent of input sampling).
    pub last_dab_position: Vec3,
    /// Previous input position, independent of the last emitted dab.
    pub last_input_position: Vec3,
    /// Arc length accumulated since the last dab, across input segments.
    pub distance_since_dab: f32,
    /// Immutable, fixed-point-representable origin of the current packet.
    pub base_position: Vec3,
    /// Exact integer coordinates written to the current packet header.
    pub base_coordinates: [i32; 3],
    /// Previous decoded position, used to prevent cumulative quantization drift.
    pub encoded_position: Vec3,
    /// Current packet dabs
    pub current_dabs: Vec<SculptDab>,
    /// Completed packets
    pub completed_packets: Vec<SculptStrokePacket>,
}

impl StrokeState {
    /// Create a new stroke state.
    pub fn new(stroke_id: u64, mesh_id: u32, start_position: Vec3, timestamp_ms: u64) -> Self {
        let base_coordinates = start_position
            .to_array()
            .map(|x| (x * 1000.0).round() as i32);
        let base_position = Vec3::from_array(base_coordinates.map(|x| x as f32)) / 1000.0;
        Self {
            stroke_id,
            mesh_id,
            start_time_ms: timestamp_ms,
            last_dab_position: start_position,
            last_input_position: start_position,
            distance_since_dab: 0.0,
            base_position,
            base_coordinates,
            encoded_position: base_position,
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
        if !input.position.is_finite() || !input.normal.is_finite() || !input.pressure.is_finite() {
            return Vec::new();
        }
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
            stroke.last_input_position = input.position;
            stroke.distance_since_dab = 0.0;
            self.active_stroke = Some(stroke);
            return results;
        }

        // Consume each input segment once. Residual travel is an arc length,
        // so a turn follows the new segment rather than a chord from the last dab.
        let start = stroke.last_input_position;
        let length = input.position.distance(start) as f64;
        if !length.is_finite() || !spacing_distance.is_finite() {
            self.active_stroke = Some(stroke);
            return results;
        }
        stroke.last_input_position = input.position;
        let spacing = spacing_distance as f64;
        let mut residual = stroke.distance_since_dab as f64;
        let mut consumed = 0.0;
        // Float input subdivision can differ by a few ULPs at a spacing boundary.
        // Snap only to the current segment's endpoint, never beyond input.
        let epsilon = spacing * f32::EPSILON as f64 * 4.0;
        while residual + (length - consumed) + epsilon >= spacing {
            let step = (spacing - residual).max(0.0);
            consumed = (consumed + step).min(length);
            let t = if length > 0.0 {
                (consumed / length) as f32
            } else {
                1.0
            };
            let position = start.lerp(input.position, t);
            let dab_input = BrushInput { position, ..input };
            results.push(self.create_dab(&mut stroke, dab_input));
            stroke.last_dab_position = position;
            residual = 0.0;
            if consumed >= length {
                break;
            }
        }
        stroke.distance_since_dab = (residual + length - consumed) as f32;

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
        // Deltas are relative to the previous *decoded* point. The packet's
        // header origin stays fixed until that packet is finalized.
        let mut scaled_delta =
            ((input.position - stroke.encoded_position) * self.delta_scale).round();
        let needs_new_packet = scaled_delta.abs().max_element() > 127.0;
        if needs_new_packet {
            if !stroke.current_dabs.is_empty() {
                stroke.completed_packets.push(self.create_packet(stroke));
                stroke.current_dabs.clear();
            }
            // This also handles an oversized first jump without emitting an
            // empty packet or saturating the first dab's delta.
            stroke.base_coordinates = input
                .position
                .to_array()
                .map(|x| (x * 1000.0).round() as i32);
            stroke.base_position =
                Vec3::from_array(stroke.base_coordinates.map(|x| x as f32)) / 1000.0;
            stroke.encoded_position = stroke.base_position;
            scaled_delta = ((input.position - stroke.encoded_position) * self.delta_scale).round();
        }

        let dab = SculptDab {
            dx: scaled_delta.x as i8,
            dy: scaled_delta.y as i8,
            dz: scaled_delta.z as i8,
            pressure: (input.pressure * 255.0) as u8,
            radius_scale: SculptDab::encode_radius_scale(
                self.preset.effective_radius(input.pressure) / self.preset.radius,
            ),
            normal_hint: SculptDab::encode_normal(input.normal),
            _padding: [0, 0],
        };

        stroke.current_dabs.push(dab);

        stroke.encoded_position +=
            Vec3::new(dab.dx as f32, dab.dy as f32, dab.dz as f32) / self.delta_scale;

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
        // The integer origin is authoritative: re-quantizing its decoded float
        // is not idempotent at all representable coordinate magnitudes.
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
                base_x: stroke.base_coordinates[0],
                base_y: stroke.base_coordinates[1],
                base_z: stroke.base_coordinates[2],
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
    fn normal_hint_packet_preserves_input_direction_and_v1_bytes() {
        let mut engine = SculptBrushEngine::new(BrushPreset {
            spacing: 0.0,
            ..BrushPreset::default()
        });
        // Stationary inputs and zero spacing isolate normal encoding from the
        // resampler and packet-origin bookkeeping, which this test does not qualify.
        let mut input = BrushInput {
            position: Vec3::new(1.0, 2.0, 3.0),
            normal: Vec3::X,
            pressure: 1.0,
            timestamp_ms: 10,
        };
        let stroke_id = engine.begin_stroke(7, input);
        let normals = [Vec3::X, Vec3::Y, Vec3::new(1.0, 2.0, 3.0).normalize()];
        let expected_hints = [0x88, 0x8c, 0x3a];
        for (normal, expected_hint) in normals.into_iter().zip(expected_hints) {
            input.normal = normal;
            let results = engine.update_stroke(input);
            assert_eq!(results.len(), 1);
            // Live deformation still receives the original normal, not decode_normal.
            assert_eq!(results[0].normal, normal);
            assert_eq!(results[0].dab.normal_hint, expected_hint);
        }
        let packets = engine.end_stroke().expect("active stroke");
        assert_eq!(packets.len(), 1);
        let packet = &packets[0];
        assert_eq!(packet.header.version, 1);
        assert_eq!(packet.header.mesh_id, 7);
        assert_eq!(packet.header.stroke_id, stroke_id);
        assert_eq!(packet.dabs.len(), normals.len());
        for ((dab, normal), expected_hint) in packet.dabs.iter().zip(normals).zip(expected_hints) {
            assert_eq!(
                bytemuck::bytes_of(dab),
                &[0, 0, 0, 255, 85, expected_hint, 0, 0]
            );
            let decoded = dab.decode_normal();
            let error = normal.dot(decoded).clamp(-1.0, 1.0).acos();
            // One polar bin plus one azimuth bin is a conservative angular bound.
            let bound = std::f32::consts::PI / 16.0 + std::f32::consts::TAU / 16.0 + 1e-5;
            assert!(error <= bound, "normal {normal:?}, error {error}");
        }
    }

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
    fn test_falloff_with_hardness() {
        // With hardness=0.0, should behave identically to evaluate()
        for curve in [
            FalloffCurve::Linear,
            FalloffCurve::Smooth,
            FalloffCurve::Sharp,
            FalloffCurve::Sphere,
        ] {
            assert!((curve.evaluate_with_hardness(0.0, 0.0) - curve.evaluate(0.0)).abs() < 0.001);
            assert!((curve.evaluate_with_hardness(0.5, 0.0) - curve.evaluate(0.5)).abs() < 0.001);
            assert!((curve.evaluate_with_hardness(1.0, 0.0) - curve.evaluate(1.0)).abs() < 0.001);
        }

        // With hardness=1.0, everything should be full strength
        for curve in [
            FalloffCurve::Linear,
            FalloffCurve::Smooth,
            FalloffCurve::Sharp,
            FalloffCurve::Sphere,
        ] {
            assert!((curve.evaluate_with_hardness(0.0, 1.0) - 1.0).abs() < 0.001);
            assert!((curve.evaluate_with_hardness(0.5, 1.0) - 1.0).abs() < 0.001);
            assert!((curve.evaluate_with_hardness(1.0, 1.0) - 1.0).abs() < 0.001);
        }

        // With hardness=0.5, inner half should be full strength
        assert!((FalloffCurve::Linear.evaluate_with_hardness(0.0, 0.5) - 1.0).abs() < 0.001);
        assert!((FalloffCurve::Linear.evaluate_with_hardness(0.5, 0.5) - 1.0).abs() < 0.001);
        // At d=0.75 with hardness=0.5: remapped = (0.75-0.5)/0.5 = 0.5, linear(0.5) = 0.5
        assert!((FalloffCurve::Linear.evaluate_with_hardness(0.75, 0.5) - 0.5).abs() < 0.001);
        // At edge: remapped = 1.0, linear(1.0) = 0.0
        assert!((FalloffCurve::Linear.evaluate_with_hardness(1.0, 0.5) - 0.0).abs() < 0.001);
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

#[cfg(test)]
mod sampling_regression_tests {
    use super::*;

    fn input(position: Vec3) -> BrushInput {
        BrushInput {
            position,
            normal: Vec3::Z,
            pressure: 1.,
            timestamp_ms: 1,
        }
    }

    fn engine(spacing: f32) -> SculptBrushEngine {
        SculptBrushEngine::new(BrushPreset {
            radius: 1.,
            spacing,
            ..Default::default()
        })
    }

    fn decode_positions(packets: &[SculptStrokePacket]) -> Vec<Vec3> {
        packets
            .iter()
            .flat_map(|packet| {
                let h = &packet.header;
                let mut position =
                    Vec3::new(h.base_x as f32, h.base_y as f32, h.base_z as f32) / 1000.;
                packet.dabs.iter().map(move |dab| {
                    position += Vec3::new(dab.dx as f32, dab.dy as f32, dab.dz as f32) / 100.;
                    position
                })
            })
            .collect()
    }

    #[test]
    fn short_segments_never_emit_a_dab_ahead_of_input() {
        let mut engine = engine(0.1);
        engine.begin_stroke(1, input(Vec3::ZERO));
        for x in [0.02, 0.04, 0.06, 0.08] {
            assert!(
                engine.update_stroke(input(Vec3::X * x)).is_empty(),
                "sample {x}"
            );
        }
        let dabs = engine.update_stroke(input(Vec3::X * 0.1));
        assert_eq!(dabs.len(), 1);
        assert!((dabs[0].position - Vec3::X * 0.1).length() < 1e-6);
    }

    #[test]
    fn stationary_samples_do_not_recount_travel() {
        let mut engine = engine(0.1);
        engine.begin_stroke(1, input(Vec3::ZERO));
        for _ in 0..20 {
            assert!(engine.update_stroke(input(Vec3::X * 0.02)).is_empty());
        }
    }

    #[test]
    fn resampling_is_invariant_to_straight_segment_subdivision() {
        fn sample(points: &[f32]) -> Vec<Vec3> {
            let mut engine = engine(0.1);
            engine.begin_stroke(1, input(Vec3::ZERO));
            points
                .iter()
                .flat_map(|&x| engine.update_stroke(input(Vec3::X * x)))
                .map(|dab| dab.position)
                .collect()
        }
        let coarse = sample(&[1.]);
        let fine = sample(&(1..=100).map(|i| i as f32 / 100.).collect::<Vec<_>>());
        assert_eq!(coarse.len(), 10);
        assert_eq!(fine.len(), coarse.len());
        for (a, b) in coarse.iter().zip(fine) {
            assert!(a.distance(b) < 1e-6);
        }
    }

    #[test]
    fn residual_travel_follows_the_actual_polyline_corner() {
        let mut engine = engine(0.1);
        engine.begin_stroke(1, input(Vec3::ZERO));
        assert!(
            engine
                .update_stroke(input(Vec3::new(0.06, 0., 0.)))
                .is_empty()
        );
        let dabs = engine.update_stroke(input(Vec3::new(0.06, 0.06, 0.)));
        assert_eq!(dabs.len(), 1);
        assert!(dabs[0].position.distance(Vec3::new(0.06, 0.04, 0.)) < 1e-6);
    }

    #[test]
    fn packet_origin_precedes_its_deltas() {
        let mut engine = engine(0.);
        engine.begin_stroke(1, input(Vec3::ZERO));
        engine.update_stroke(input(Vec3::X * 0.1));
        engine.update_stroke(input(Vec3::X * 0.2));
        let packets = engine.end_stroke().unwrap();
        assert_eq!(packets[0].header.base_x, 0);
        let positions = decode_positions(&packets);
        assert!(positions[0].distance(Vec3::X * 0.1) < 1e-6);
        assert!(positions[1].distance(Vec3::X * 0.2) < 1e-6);
    }

    #[test]
    fn first_large_jump_starts_a_packet_without_saturating_the_delta() {
        let mut engine = engine(0.);
        engine.begin_stroke(1, input(Vec3::ZERO));
        let live = engine.update_stroke(input(Vec3::new(10., -20., 30.)));
        let packets = engine.end_stroke().unwrap();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].dabs.len(), 1);
        assert!(decode_positions(&packets)[0].distance(live[0].position) < 0.009);
    }

    #[test]
    fn packet_overflow_preserves_all_dab_positions_and_nonempty_packets() {
        let mut engine = engine(0.);
        engine.begin_stroke(1, input(Vec3::new(0.123, 0., 0.)));
        let positions = [0.2, 0.4, 3.5, 3.7, -2., -2.2];
        for &x in &positions {
            engine.update_stroke(input(Vec3::X * x));
        }
        let packets = engine.end_stroke().unwrap();
        assert_eq!(packets.len(), 3);
        assert!(packets.iter().all(|packet| !packet.dabs.is_empty()));
        let decoded = decode_positions(&packets);
        assert_eq!(decoded.len(), positions.len());
        for (actual, expected) in decoded.into_iter().zip(positions) {
            assert!(actual.distance(Vec3::X * expected) <= 0.009);
        }
    }

    #[test]
    fn sub_quantum_motion_does_not_accumulate_replay_drift() {
        let mut engine = engine(0.);
        engine.begin_stroke(1, input(Vec3::ZERO));
        let positions: Vec<_> = (1..=1000)
            .map(|i| Vec3::new(i as f32 * 0.003, -i as f32 * 0.002, i as f32 * 0.001))
            .collect();
        for &position in &positions {
            engine.update_stroke(input(position));
        }
        let decoded = decode_positions(&engine.end_stroke().unwrap());
        assert_eq!(decoded.len(), positions.len());
        for (actual, expected) in decoded.into_iter().zip(positions) {
            assert!(
                (actual - expected).abs().max_element() <= 0.0051,
                "{actual:?} vs {expected:?}"
            );
        }
    }

    #[test]
    fn empty_and_cancelled_strokes_do_not_leak_packets() {
        let mut engine = engine(0.);
        engine.begin_stroke(1, input(Vec3::ZERO));
        assert!(engine.end_stroke().unwrap().is_empty());
        engine.begin_stroke(1, input(Vec3::ZERO));
        engine.update_stroke(input(Vec3::ONE));
        engine.cancel_stroke();
        assert!(engine.end_stroke().is_none());
        engine.begin_stroke(1, input(Vec3::splat(4.)));
        engine.update_stroke(input(Vec3::splat(4.1)));
        let packets = engine.end_stroke().unwrap();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].dabs.len(), 1);
        assert!(
            (decode_positions(&packets)[0] - Vec3::splat(4.1))
                .abs()
                .max_element()
                < 0.0051
        );
    }

    #[test]
    fn header_and_cursor_use_the_same_once_quantized_origin() {
        let start = Vec3::X * f32::from_bits(0x46013296);
        let target = Vec3::X * f32::from_bits(0x46013269);
        let mut engine = engine(0.);
        engine.begin_stroke(1, input(start));
        engine.update_stroke(input(target));
        let packets = engine.end_stroke().unwrap();
        assert_eq!(packets[0].header.base_x, 8_268_647);
        assert!((decode_positions(&packets)[0] - target).abs().max_element() <= 0.0051);
        // The same rule applies after an overflow creates a fresh packet origin.
        let mut engine = self::engine(0.);
        engine.begin_stroke(1, input(Vec3::ZERO));
        engine.update_stroke(input(start));
        engine.update_stroke(input(target));
        let packets = engine.end_stroke().unwrap();
        assert_eq!(packets[0].header.base_x, 8_268_647);
        assert!((decode_positions(&packets)[1] - target).abs().max_element() <= 0.0051);
    }

    #[test]
    fn reduced_spacing_consumes_old_residual_once_at_segment_start() {
        let mut engine = engine(0.1);
        engine.preset.pressure_affects_radius = true;
        engine.begin_stroke(1, input(Vec3::ZERO));
        assert!(engine.update_stroke(input(Vec3::X * 0.09)).is_empty());
        let low_pressure = |x| BrushInput {
            pressure: 0.,
            ..input(Vec3::X * x)
        };
        let dabs = engine.update_stroke(low_pressure(0.09));
        assert_eq!(dabs.len(), 1);
        assert_eq!(dabs[0].position, Vec3::X * 0.09);
        assert!(engine.update_stroke(low_pressure(0.11)).is_empty());
        let dabs = engine.update_stroke(low_pressure(0.14));
        assert_eq!(dabs.len(), 1);
        assert!(dabs[0].position.distance(Vec3::X * 0.14) < 1e-6);
    }
}
