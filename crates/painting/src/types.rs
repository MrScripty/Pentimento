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
