use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// Space type for stroke targeting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SpaceKind {
    CanvasPlane = 0,
    MeshPtex = 1,
}

/// Blend modes for painting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Erase = 1,
    // Add more as needed
}

/// Pressure/speed quantization level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum Quantization {
    None = 0,
    #[default]
    U8 = 1,
    U16 = 2,
}

/// Header for a stroke packet (per stroke or per packet after delta overflow)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrokeHeader {
    /// Schema version
    pub version: u8,
    /// Target space type
    pub space_kind: SpaceKind,
    /// plane_id OR mesh_id
    pub space_id: u32,
    /// Unique stroke identifier (for future undo/selective replay)
    pub stroke_id: u64,
    /// Timestamp in milliseconds (for Iroh ordering)
    pub timestamp_ms: u64,
    /// Brush preset id (libmypaint)
    pub tool_id: u32,
    /// Blend mode
    pub blend_mode: BlendMode,
    /// Color in wgpu-native Rgba16Float compatible format [r, g, b, a]
    pub color: [f32; 4],
    /// Reserved flags for future (tilt, jitter, etc.)
    pub flags: u8,
    /// Base position for delta compression (fixed-point x4)
    pub base_x: i32,
    /// Base position for delta compression (fixed-point x4)
    pub base_y: i32,
    /// Mesh face id (MeshPtex only, ignored for CanvasPlane)
    pub face_id: u32,
    /// Tile index within face (MeshPtex only)
    pub ptex_tile: u16,
    /// Pressure quantization level
    pub pressure_quant: Quantization,
    /// Speed quantization level
    pub speed_quant: Quantization,
}

/// A single dab in a stroke
///
/// This struct is designed for GPU compatibility with bytemuck.
/// Field order is arranged for proper alignment (largest fields first).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Dab {
    /// Brush diameter * 256
    pub size: u32,
    /// Pressure (stored as u16, interpret based on header quantization)
    pub pressure: u16,
    /// Speed (stored as u16, interpret based on header quantization)
    pub speed: u16,
    /// Delta x from previous dab (x4 units, +/-127 max)
    pub dx: i8,
    /// Delta y from previous dab (x4 units, +/-127 max)
    pub dy: i8,
    /// Hardness 0..255
    pub hardness: u8,
    /// Opacity 0..255
    pub opacity: u8,
    /// Angle 0..255 (maps to 0..255)
    pub angle: u8,
    /// Aspect ratio 0..255
    pub aspect_ratio: u8,
    /// Padding for 4-byte alignment
    pub _padding: [u8; 2],
}

/// A complete stroke packet (header + dabs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrokePacket {
    pub header: StrokeHeader,
    pub dabs: Vec<Dab>,
}

// ============================================================================
// Mesh Painting Types
// ============================================================================

/// Hit information for mesh painting.
///
/// Contains all the geometric data needed to paint at a point on a mesh surface,
/// including interpolated surface properties from the triangle vertices.
#[derive(Debug, Clone)]
pub struct MeshHit {
    /// World-space position of the hit point
    pub world_pos: Vec3,
    /// Face (triangle) index in the mesh
    pub face_id: u32,
    /// Barycentric coordinates within the triangle (u, v, w where w = 1-u-v)
    pub barycentric: Vec3,
    /// Interpolated surface normal at the hit point (world space, normalized)
    pub normal: Vec3,
    /// Tangent vector for tangent-space basis (world space, normalized)
    pub tangent: Vec3,
    /// Bitangent vector (cross of normal and tangent, world space, normalized)
    pub bitangent: Vec3,
    /// Interpolated UV coordinate (if mesh has UVs, otherwise None)
    pub uv: Option<Vec2>,
}

/// Result of projecting a brush onto a surface tangent plane.
///
/// When painting on a 3D surface, the brush must be projected from world space
/// onto the surface's tangent plane to avoid distortion at oblique angles.
#[derive(Debug, Clone, Copy)]
pub struct ProjectedDab {
    /// Position in texture space (UV coordinates or Ptex face-local coords)
    pub tex_pos: Vec2,
    /// Size in texture pixels (after projection accounting for surface angle)
    pub size: f32,
    /// Rotation angle in radians (from tangent alignment)
    pub angle: f32,
    /// Aspect ratio: 1.0 = circular on surface, <1.0 = ellipse stretched along minor axis
    pub aspect_ratio: f32,
}

/// Storage mode for a paintable mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshStorageMode {
    /// Paint to UV texture atlas (requires mesh UVs)
    UvAtlas {
        /// Texture resolution (width, height)
        resolution: (u32, u32),
    },
    /// Per-face textures for UV-less meshes (Ptex-style)
    Ptex {
        /// Resolution of each face's texture tile
        face_resolution: u32,
    },
}
