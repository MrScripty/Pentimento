//! Stroke recorder for building strokes with delta overflow handling.

use crate::types::{BlendMode, Dab, Quantization, SpaceKind, StrokeHeader, StrokePacket};
use crate::validation::{compute_delta, to_fixed_point, validate_dab, ValidationError};

use super::dab_params::DabParams;
use super::events::StrokeLogEvent;

/// Error type for stroke recording operations.
#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("Stroke not started - call start() first")]
    NotStarted,
    #[error("Stroke already started - call finish() or abort() first")]
    AlreadyStarted,
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("No dabs recorded")]
    NoDabs,
}

/// Configuration for starting a new stroke.
#[derive(Debug, Clone)]
pub struct StrokeConfig {
    pub space_kind: SpaceKind,
    pub space_id: u32,
    pub stroke_id: u64,
    pub timestamp_ms: u64,
    pub tool_id: u32,
    pub blend_mode: BlendMode,
    pub color: [f32; 4],
    pub flags: u8,
    pub face_id: u32,
    pub ptex_tile: u16,
    pub pressure_quant: Quantization,
    pub speed_quant: Quantization,
}

impl Default for StrokeConfig {
    fn default() -> Self {
        Self {
            space_kind: SpaceKind::CanvasPlane,
            space_id: 0,
            stroke_id: 0,
            timestamp_ms: 0,
            tool_id: 0,
            blend_mode: BlendMode::default(),
            color: [0.0, 0.0, 0.0, 1.0],
            flags: 0,
            face_id: 0,
            ptex_tile: 0,
            pressure_quant: Quantization::default(),
            speed_quant: Quantization::default(),
        }
    }
}

/// Helper for building strokes with automatic delta overflow handling.
///
/// The recorder tracks the current stroke state and handles:
/// - Delta compression between consecutive dabs
/// - Automatic packet flushing when delta overflows i8 range
/// - Dab validation before adding
///
/// When delta overflow occurs, the current packet is completed and a new
/// packet is started with updated base_x/base_y coordinates. All packets
/// share the same stroke_id.
///
/// # Example
///
/// ```ignore
/// let mut recorder = StrokeRecorder::new();
/// recorder.start(config, 100.0, 100.0)?;
/// recorder.add_dab(101.0, 101.0, dab_params)?;
/// recorder.add_dab(102.0, 102.0, dab_params)?;
/// let packets = recorder.finish()?;
/// for packet in packets {
///     stroke_log.append(packet);
/// }
/// ```
#[derive(Debug, Default)]
pub struct StrokeRecorder {
    /// Current stroke configuration (None if not recording)
    config: Option<StrokeConfig>,
    /// Current base position in fixed-point (x4 units)
    base_x: i32,
    base_y: i32,
    /// Last dab position in fixed-point for delta computation
    last_x: i32,
    last_y: i32,
    /// Dabs accumulated in current packet
    current_dabs: Vec<Dab>,
    /// Completed packets (from delta overflow)
    completed_packets: Vec<StrokePacket>,
    /// Whether this is the first dab (no delta needed)
    is_first_dab: bool,
}

impl StrokeRecorder {
    /// Create a new stroke recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if currently recording a stroke.
    pub fn is_recording(&self) -> bool {
        self.config.is_some()
    }

    /// Start recording a new stroke.
    ///
    /// The initial position sets the base coordinates for delta compression.
    /// Returns a `StrokeStarted` event that should be emitted to listeners.
    pub fn start(
        &mut self,
        config: StrokeConfig,
        start_x: f32,
        start_y: f32,
    ) -> Result<StrokeLogEvent, RecorderError> {
        if self.config.is_some() {
            return Err(RecorderError::AlreadyStarted);
        }

        let fixed_x = to_fixed_point(start_x);
        let fixed_y = to_fixed_point(start_y);

        self.base_x = fixed_x;
        self.base_y = fixed_y;
        self.last_x = fixed_x;
        self.last_y = fixed_y;
        self.current_dabs.clear();
        self.completed_packets.clear();
        self.is_first_dab = true;

        let event = StrokeLogEvent::StrokeStarted {
            stroke_id: config.stroke_id,
            space_id: config.space_id,
            timestamp_ms: config.timestamp_ms,
        };

        self.config = Some(config);

        Ok(event)
    }

    /// Add a dab to the current stroke.
    ///
    /// If the delta from the previous dab exceeds i8 range, the current packet
    /// is automatically flushed and a new packet is started with updated base
    /// coordinates.
    ///
    /// The dab is validated before being added.
    pub fn add_dab(&mut self, x: f32, y: f32, params: DabParams) -> Result<(), RecorderError> {
        if self.config.is_none() {
            return Err(RecorderError::NotStarted);
        }

        let fixed_x = to_fixed_point(x);
        let fixed_y = to_fixed_point(y);

        // Compute deltas from last position
        let (dx, dy) = if self.is_first_dab {
            // First dab: delta from base position (should be 0,0 if start position matches)
            let dx = compute_delta(self.base_x, fixed_x);
            let dy = compute_delta(self.base_y, fixed_y);
            self.is_first_dab = false;
            (dx, dy)
        } else {
            let dx = compute_delta(self.last_x, fixed_x);
            let dy = compute_delta(self.last_y, fixed_y);
            (dx, dy)
        };

        // Check for delta overflow
        if dx.is_none() || dy.is_none() {
            // Flush current packet and start new one with new base
            self.flush_current_packet();

            // New base is the current position
            self.base_x = fixed_x;
            self.base_y = fixed_y;
            self.is_first_dab = true;

            // First dab in new packet has 0,0 delta
            let dab = Dab {
                dx: 0,
                dy: 0,
                size: params.size,
                pressure: params.pressure,
                speed: params.speed,
                hardness: params.hardness,
                opacity: params.opacity,
                angle: params.angle,
                aspect_ratio: params.aspect_ratio,
                _padding: [0, 0],
            };

            validate_dab(&dab)?;
            self.current_dabs.push(dab);
            self.is_first_dab = false;
        } else {
            let dab = Dab {
                dx: dx.unwrap(),
                dy: dy.unwrap(),
                size: params.size,
                pressure: params.pressure,
                speed: params.speed,
                hardness: params.hardness,
                opacity: params.opacity,
                angle: params.angle,
                aspect_ratio: params.aspect_ratio,
                _padding: [0, 0],
            };

            validate_dab(&dab)?;
            self.current_dabs.push(dab);
        }

        self.last_x = fixed_x;
        self.last_y = fixed_y;

        Ok(())
    }

    /// Flush the current dabs into a completed packet.
    ///
    /// Called automatically on delta overflow, or can be called manually.
    fn flush_current_packet(&mut self) {
        if self.current_dabs.is_empty() {
            return;
        }

        let config = self.config.as_ref().expect("flush called without config");

        let header = StrokeHeader {
            version: 1,
            space_kind: config.space_kind,
            space_id: config.space_id,
            stroke_id: config.stroke_id,
            timestamp_ms: config.timestamp_ms,
            tool_id: config.tool_id,
            blend_mode: config.blend_mode,
            color: config.color,
            flags: config.flags,
            base_x: self.base_x,
            base_y: self.base_y,
            face_id: config.face_id,
            ptex_tile: config.ptex_tile,
            pressure_quant: config.pressure_quant,
            speed_quant: config.speed_quant,
        };

        let packet = StrokePacket {
            header,
            dabs: std::mem::take(&mut self.current_dabs),
        };

        self.completed_packets.push(packet);
    }

    /// Finish recording and return all completed stroke packets.
    ///
    /// Returns an error if no dabs were recorded.
    pub fn finish(&mut self) -> Result<Vec<StrokePacket>, RecorderError> {
        if self.config.is_none() {
            return Err(RecorderError::NotStarted);
        }

        // Flush any remaining dabs
        self.flush_current_packet();

        if self.completed_packets.is_empty() {
            self.config = None;
            return Err(RecorderError::NoDabs);
        }

        self.config = None;
        Ok(std::mem::take(&mut self.completed_packets))
    }

    /// Abort the current stroke without completing it.
    ///
    /// Returns a `StrokeAborted` event that should be emitted to listeners.
    pub fn abort(&mut self, reason: String) -> Result<StrokeLogEvent, RecorderError> {
        let config = self.config.take().ok_or(RecorderError::NotStarted)?;

        self.current_dabs.clear();
        self.completed_packets.clear();

        Ok(StrokeLogEvent::StrokeAborted {
            stroke_id: config.stroke_id,
            reason,
        })
    }

    /// Get the current stroke_id if recording.
    pub fn stroke_id(&self) -> Option<u64> {
        self.config.as_ref().map(|c| c.stroke_id)
    }

    /// Get the number of dabs in the current packet.
    pub fn current_dab_count(&self) -> usize {
        self.current_dabs.len()
    }

    /// Get the number of completed packets (from overflow).
    pub fn completed_packet_count(&self) -> usize {
        self.completed_packets.len()
    }
}
