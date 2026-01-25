use crate::constants::MAX_XY_DELTA;
use crate::types::Dab;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Delta overflow: dx={dx}, dy={dy} exceeds +/-{}", MAX_XY_DELTA)]
    DeltaOverflow { dx: i8, dy: i8 },
    #[error("Invalid dab size: {0}")]
    InvalidSize(u32),
    #[error("Invalid opacity: {0}")]
    InvalidOpacity(u8),
}

/// Check if a coordinate delta fits in i8 range
pub fn can_delta(last: i32, current: i32) -> bool {
    let delta = current - last;
    delta >= i8::MIN as i32 && delta <= i8::MAX as i32
}

/// Compute delta, returns None if overflow
pub fn compute_delta(last: i32, current: i32) -> Option<i8> {
    let delta = current - last;
    if delta >= i8::MIN as i32 && delta <= i8::MAX as i32 {
        Some(delta as i8)
    } else {
        None
    }
}

/// Validate a dab's parameters
pub fn validate_dab(dab: &Dab) -> Result<(), ValidationError> {
    // Size must be non-zero (diameter * 256, so 256 = 1 pixel)
    if dab.size == 0 {
        return Err(ValidationError::InvalidSize(dab.size));
    }

    // All other fields (hardness, opacity, angle, aspect_ratio) are u8
    // so they're always in valid range 0..255

    Ok(())
}

/// Convert world coordinates to fixed-point (multiply by COORD_SCALE)
pub fn to_fixed_point(coord: f32) -> i32 {
    (coord * crate::constants::COORD_SCALE) as i32
}

/// Convert fixed-point to world coordinates (divide by COORD_SCALE)
pub fn from_fixed_point(fixed: i32) -> f32 {
    fixed as f32 / crate::constants::COORD_SCALE
}

/// Convert brush diameter to size field (multiply by SIZE_SCALE)
pub fn to_size_field(diameter: f32) -> u32 {
    (diameter * crate::constants::SIZE_SCALE) as u32
}

/// Convert size field to brush diameter (divide by SIZE_SCALE)
pub fn from_size_field(size: u32) -> f32 {
    size as f32 / crate::constants::SIZE_SCALE
}
