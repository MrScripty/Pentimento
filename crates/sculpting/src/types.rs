//! Core sculpting types.
//!
//! These types are designed for P2P sync compatibility, following the same
//! delta-compressed dab pattern as the painting system.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Type of sculpting deformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum DeformationType {
    /// Push vertices along surface normal
    #[default]
    Push = 0,
    /// Pull vertices toward brush center
    Pull = 1,
    /// Move vertices along stroke direction
    Grab = 2,
    /// Smooth vertex positions with neighbors
    Smooth = 3,
    /// Flatten vertices to average plane
    Flatten = 4,
    /// Inflate vertices along their normals
    Inflate = 5,
    /// Pinch vertices toward brush center
    Pinch = 6,
    /// Crease along stroke path
    Crease = 7,
}

/// Header for a sculpt stroke packet.
///
/// Contains metadata for a stroke and base position for delta compression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SculptStrokeHeader {
    /// Schema version
    pub version: u8,
    /// Mesh ID being sculpted
    pub mesh_id: u32,
    /// Unique stroke identifier
    pub stroke_id: u64,
    /// Timestamp in milliseconds (for ordering)
    pub timestamp_ms: u64,
    /// Deformation type for this stroke
    pub deformation_type: DeformationType,
    /// Base brush radius (world units × 1000)
    pub base_radius: u32,
    /// Brush strength 0-255
    pub strength: u8,
    /// Reserved flags
    pub flags: u8,
    /// Base position for delta compression (fixed-point ×1000)
    pub base_x: i32,
    pub base_y: i32,
    pub base_z: i32,
}

/// A single sculpt dab.
///
/// Uses delta compression for position (relative to previous dab or base).
/// When deltas exceed i8 range, a new packet is created with updated base.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct SculptDab {
    /// Delta x from previous dab (×100 scale, +/-127 max)
    pub dx: i8,
    /// Delta y from previous dab (×100 scale, +/-127 max)
    pub dy: i8,
    /// Delta z from previous dab (×100 scale, +/-127 max)
    pub dz: i8,
    /// Pressure 0-255
    pub pressure: u8,
    /// Radius scale 0-255 (maps to 0.5x-2.0x)
    pub radius_scale: u8,
    /// Quantized normal direction hint
    pub normal_hint: u8,
    /// Padding for 8-byte alignment
    pub _padding: [u8; 2],
}

impl SculptDab {
    /// Decode radius scale to multiplier (0.5 to 2.0).
    pub fn radius_multiplier(&self) -> f32 {
        0.5 + (self.radius_scale as f32 / 255.0) * 1.5
    }

    /// Encode radius multiplier (0.5 to 2.0) to scale byte.
    pub fn encode_radius_scale(multiplier: f32) -> u8 {
        let clamped = multiplier.clamp(0.5, 2.0);
        ((clamped - 0.5) / 1.5 * 255.0) as u8
    }

    /// Decode normal hint to unit vector (quantized to 256 directions).
    pub fn decode_normal(&self) -> Vec3 {
        // Simple sphere mapping: hint encodes phi (0-15) and theta (0-15)
        let phi_idx = (self.normal_hint >> 4) as f32;
        let theta_idx = (self.normal_hint & 0x0F) as f32;

        let phi = phi_idx / 16.0 * std::f32::consts::PI;
        let theta = theta_idx / 16.0 * std::f32::consts::TAU;

        Vec3::new(
            phi.sin() * theta.cos(),
            phi.sin() * theta.sin(),
            phi.cos(),
        )
    }

    /// Encode unit normal to hint byte.
    pub fn encode_normal(normal: Vec3) -> u8 {
        let n = normal.normalize_or_zero();
        let phi = n.z.clamp(-1.0, 1.0).acos();
        let theta = n.y.atan2(n.x);

        let phi_idx = ((phi / std::f32::consts::PI) * 16.0) as u8;
        let theta_idx = (((theta + std::f32::consts::PI) / std::f32::consts::TAU) * 16.0) as u8;

        (phi_idx.min(15) << 4) | theta_idx.min(15)
    }
}

/// A complete sculpt stroke packet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SculptStrokePacket {
    pub header: SculptStrokeHeader,
    pub dabs: Vec<SculptDab>,
}

/// Configuration for tessellation behavior.
///
/// Values are configurable and should not be treated as magic numbers.
#[derive(Debug, Clone)]
pub struct TessellationConfig {
    /// Target edge length in screen pixels (default: 6.0)
    pub target_pixels: f32,
    /// Split edges larger than target × split_ratio (default: 1.5)
    pub split_ratio: f32,
    /// Collapse edges smaller than target × collapse_ratio (default: 0.4)
    pub collapse_ratio: f32,
}

impl Default for TessellationConfig {
    fn default() -> Self {
        Self {
            target_pixels: 6.0,
            split_ratio: 1.5,
            collapse_ratio: 0.4,
        }
    }
}

/// Configuration for mesh chunk sizing.
///
/// Values are configurable and should not be treated as magic numbers.
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Below this face count, consider merging with neighbor (default: 5000)
    pub min_faces: usize,
    /// Above this face count, consider splitting (default: 15000)
    pub max_faces: usize,
    /// Ideal working size (default: 10000)
    pub target_faces: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            min_faces: 5000,
            max_faces: 15000,
            target_faces: 10000,
        }
    }
}

/// Action determined by tessellation evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TessellationAction {
    /// Edge should be split (subdivided)
    Split,
    /// Edge should be collapsed (merged)
    Collapse,
    /// No action needed
    None,
}
