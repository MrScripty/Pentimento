//! Dab parameters for stroke recording.

/// Parameters for a single dab (excluding position, which is passed separately).
#[derive(Debug, Clone, Copy)]
pub struct DabParams {
    pub size: u32,
    pub pressure: u16,
    pub speed: u16,
    pub hardness: u8,
    pub opacity: u8,
    pub angle: u8,
    pub aspect_ratio: u8,
}

impl Default for DabParams {
    fn default() -> Self {
        Self {
            size: 256, // 1 pixel diameter
            pressure: 255,
            speed: 0,
            hardness: 128,
            opacity: 255,
            angle: 0,
            aspect_ratio: 255, // circular
        }
    }
}
