/// Maximum canvas size (DCI 1K). Not a magic number - may change.
pub const MAX_CANVAS_SIZE: u32 = 1048;

/// Coordinate scale factor (1/4 pixel precision).
pub const COORD_SCALE: f32 = 4.0;

/// Size scale factor (diameter * 256).
pub const SIZE_SCALE: f32 = 256.0;

/// Maximum delta for i8 encoding.
pub const MAX_XY_DELTA: i8 = 127;

/// Default tile size for CPU surface.
pub const DEFAULT_TILE_SIZE: u32 = 128;
