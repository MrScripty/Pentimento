//! Stroke log storage and Iroh-ready hooks for the Pentimento painting system.
//!
//! This module provides:
//! - [`StrokeLog`] - Thread-safe append-only storage for stroke packets
//! - [`StrokeLogEvent`] - Events for Iroh integration hooks
//! - [`StrokeRecorder`] - Helper for building strokes with delta overflow handling
//!
//! ## Iroh Key Format
//!
//! For future Iroh integration, strokes are addressed using the following key format:
//!
//! ```text
//! strokes/{space_id}/{stroke_id}
//! ```
//!
//! Where:
//! - `space_id` is the u32 target space (plane_id or mesh_id from StrokeHeader)
//! - `stroke_id` is the u64 unique stroke identifier
//!
//! Example: `strokes/42/1705847123456789` for stroke 1705847123456789 on space 42
//!
//! When a stroke overflows delta compression limits, multiple packets share the same
//! stroke_id but have different base_x/base_y values. These are stored sequentially
//! and must be replayed in order.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::types::{BlendMode, Dab, Quantization, SpaceKind, StrokeHeader, StrokePacket};
use crate::validation::{compute_delta, to_fixed_point, validate_dab, ValidationError};

/// Events emitted during stroke recording for Iroh integration hooks.
///
/// These events allow external systems (like Iroh document sync) to react to
/// stroke lifecycle changes without tight coupling to the log implementation.
#[derive(Debug, Clone)]
pub enum StrokeLogEvent {
    /// A new stroke has started recording.
    StrokeStarted {
        stroke_id: u64,
        space_id: u32,
        timestamp_ms: u64,
    },
    /// A stroke packet has been completed and stored.
    /// Note: A single stroke may produce multiple packets if delta overflow occurs.
    StrokeCompleted { packet: StrokePacket },
    /// A stroke was aborted before completion.
    StrokeAborted { stroke_id: u64, reason: String },
}

/// Thread-safe append-only storage for stroke packets.
///
/// Designed for concurrent access from Bevy systems. Uses interior mutability
/// via RwLock to allow multiple readers or a single writer.
///
/// Strokes are indexed by space_id for efficient querying of all strokes
/// affecting a particular canvas plane or mesh.
pub struct StrokeLog {
    /// Strokes indexed by space_id for efficient queries.
    /// Uses std::sync::RwLock for thread safety.
    strokes: RwLock<HashMap<u32, Vec<StrokePacket>>>,
    /// Event listeners for Iroh integration hooks.
    /// Each listener receives cloned events.
    #[allow(clippy::type_complexity)]
    event_listeners: RwLock<Vec<Box<dyn Fn(StrokeLogEvent) + Send + Sync>>>,
}

impl std::fmt::Debug for StrokeLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stroke_count = self
            .strokes
            .read()
            .map(|s| s.values().map(|v| v.len()).sum::<usize>())
            .unwrap_or(0);
        let listener_count = self
            .event_listeners
            .read()
            .map(|l| l.len())
            .unwrap_or(0);
        f.debug_struct("StrokeLog")
            .field("stroke_count", &stroke_count)
            .field("listener_count", &listener_count)
            .finish()
    }
}

impl Default for StrokeLog {
    fn default() -> Self {
        Self {
            strokes: RwLock::new(HashMap::new()),
            event_listeners: RwLock::new(Vec::new()),
        }
    }
}

impl StrokeLog {
    /// Create a new empty stroke log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a stroke packet to the log.
    ///
    /// This is append-only; packets cannot be removed (for now).
    /// Emits a `StrokeCompleted` event to all registered listeners.
    pub fn append(&self, packet: StrokePacket) {
        let space_id = packet.header.space_id;

        // Store the packet
        {
            let mut strokes = self.strokes.write().expect("StrokeLog lock poisoned");
            strokes.entry(space_id).or_default().push(packet.clone());
        }

        // Emit event to listeners
        self.emit_event(StrokeLogEvent::StrokeCompleted { packet });
    }

    /// Query all stroke packets for a given space_id.
    ///
    /// Returns an empty Vec if no strokes exist for that space.
    pub fn query_by_space(&self, space_id: u32) -> Vec<StrokePacket> {
        let strokes = self.strokes.read().expect("StrokeLog lock poisoned");
        strokes.get(&space_id).cloned().unwrap_or_default()
    }

    /// Get the total number of stroke packets across all spaces.
    pub fn total_packet_count(&self) -> usize {
        let strokes = self.strokes.read().expect("StrokeLog lock poisoned");
        strokes.values().map(|v| v.len()).sum()
    }

    /// Get all space IDs that have strokes.
    pub fn space_ids(&self) -> Vec<u32> {
        let strokes = self.strokes.read().expect("StrokeLog lock poisoned");
        strokes.keys().copied().collect()
    }

    /// Register an event listener for Iroh integration hooks.
    ///
    /// The listener will receive cloned events for:
    /// - `StrokeStarted` - when a stroke begins recording
    /// - `StrokeCompleted` - when a stroke packet is stored
    /// - `StrokeAborted` - when a stroke is cancelled
    pub fn add_event_listener<F>(&self, listener: F)
    where
        F: Fn(StrokeLogEvent) + Send + Sync + 'static,
    {
        let mut listeners = self.event_listeners.write().expect("StrokeLog lock poisoned");
        listeners.push(Box::new(listener));
    }

    /// Emit an event to all registered listeners.
    fn emit_event(&self, event: StrokeLogEvent) {
        let listeners = self.event_listeners.read().expect("StrokeLog lock poisoned");
        for listener in listeners.iter() {
            listener(event.clone());
        }
    }

    /// Generate the Iroh key for a stroke packet.
    ///
    /// Format: `strokes/{space_id}/{stroke_id}`
    pub fn iroh_key(space_id: u32, stroke_id: u64) -> String {
        format!("strokes/{}/{}", space_id, stroke_id)
    }
}

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
    pub fn add_dab(
        &mut self,
        x: f32,
        y: f32,
        params: DabParams,
    ) -> Result<(), RecorderError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stroke_log_append_and_query() {
        let log = StrokeLog::new();

        let packet = StrokePacket {
            header: StrokeHeader {
                version: 1,
                space_kind: SpaceKind::CanvasPlane,
                space_id: 42,
                stroke_id: 1,
                timestamp_ms: 1000,
                tool_id: 0,
                blend_mode: BlendMode::Normal,
                color: [0.0, 0.0, 0.0, 1.0],
                flags: 0,
                base_x: 0,
                base_y: 0,
                face_id: 0,
                ptex_tile: 0,
                pressure_quant: Quantization::U8,
                speed_quant: Quantization::U8,
            },
            dabs: vec![Dab {
                dx: 0,
                dy: 0,
                size: 256,
                pressure: 255,
                speed: 0,
                hardness: 128,
                opacity: 255,
                angle: 0,
                aspect_ratio: 255,
                _padding: [0, 0],
            }],
        };

        log.append(packet);

        let result = log.query_by_space(42);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].header.stroke_id, 1);

        // Query non-existent space
        let empty = log.query_by_space(999);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_stroke_recorder_basic() {
        let mut recorder = StrokeRecorder::new();

        let config = StrokeConfig {
            space_id: 1,
            stroke_id: 100,
            timestamp_ms: 5000,
            ..Default::default()
        };

        // Start stroke
        let event = recorder.start(config, 10.0, 10.0).unwrap();
        matches!(event, StrokeLogEvent::StrokeStarted { stroke_id: 100, .. });

        assert!(recorder.is_recording());

        // Add dabs
        let params = DabParams::default();
        recorder.add_dab(10.0, 10.0, params).unwrap();
        recorder.add_dab(11.0, 11.0, params).unwrap();
        recorder.add_dab(12.0, 12.0, params).unwrap();

        assert_eq!(recorder.current_dab_count(), 3);

        // Finish
        let packets = recorder.finish().unwrap();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].dabs.len(), 3);
        assert_eq!(packets[0].header.stroke_id, 100);
    }

    #[test]
    fn test_stroke_recorder_delta_overflow() {
        let mut recorder = StrokeRecorder::new();

        let config = StrokeConfig {
            space_id: 1,
            stroke_id: 200,
            timestamp_ms: 6000,
            ..Default::default()
        };

        recorder.start(config, 0.0, 0.0).unwrap();

        let params = DabParams::default();

        // Add a dab at origin
        recorder.add_dab(0.0, 0.0, params).unwrap();

        // Add a dab far away (beyond i8 delta range)
        // i8 max is 127, and with COORD_SCALE=4, that's ~31.75 pixels
        // Moving 100 pixels should definitely overflow
        recorder.add_dab(100.0, 100.0, params).unwrap();

        // Should have flushed one packet already
        assert_eq!(recorder.completed_packet_count(), 1);
        assert_eq!(recorder.current_dab_count(), 1);

        let packets = recorder.finish().unwrap();
        assert_eq!(packets.len(), 2);

        // Second packet should have updated base coordinates
        let second_base_x = packets[1].header.base_x;
        let second_base_y = packets[1].header.base_y;
        assert_eq!(second_base_x, to_fixed_point(100.0));
        assert_eq!(second_base_y, to_fixed_point(100.0));
    }

    #[test]
    fn test_stroke_recorder_abort() {
        let mut recorder = StrokeRecorder::new();

        let config = StrokeConfig {
            stroke_id: 300,
            ..Default::default()
        };

        recorder.start(config, 0.0, 0.0).unwrap();
        recorder.add_dab(1.0, 1.0, DabParams::default()).unwrap();

        let event = recorder.abort("User cancelled".to_string()).unwrap();
        match event {
            StrokeLogEvent::StrokeAborted { stroke_id, reason } => {
                assert_eq!(stroke_id, 300);
                assert_eq!(reason, "User cancelled");
            }
            _ => panic!("Expected StrokeAborted event"),
        }

        assert!(!recorder.is_recording());
    }

    #[test]
    fn test_iroh_key_format() {
        let key = StrokeLog::iroh_key(42, 1705847123456789);
        assert_eq!(key, "strokes/42/1705847123456789");
    }

    #[test]
    fn test_stroke_log_event_listener() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let log = StrokeLog::new();
        let event_count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&event_count);

        log.add_event_listener(move |_event| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let packet = StrokePacket {
            header: StrokeHeader {
                version: 1,
                space_kind: SpaceKind::CanvasPlane,
                space_id: 1,
                stroke_id: 1,
                timestamp_ms: 1000,
                tool_id: 0,
                blend_mode: BlendMode::Normal,
                color: [0.0, 0.0, 0.0, 1.0],
                flags: 0,
                base_x: 0,
                base_y: 0,
                face_id: 0,
                ptex_tile: 0,
                pressure_quant: Quantization::U8,
                speed_quant: Quantization::U8,
            },
            dabs: vec![],
        };

        log.append(packet.clone());
        log.append(packet);

        assert_eq!(event_count.load(Ordering::SeqCst), 2);
    }
}
