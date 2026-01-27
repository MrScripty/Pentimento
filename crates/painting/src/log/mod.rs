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

mod dab_params;
mod events;
mod recorder;
mod storage;

pub use dab_params::DabParams;
pub use events::StrokeLogEvent;
pub use recorder::{RecorderError, StrokeConfig, StrokeRecorder};
pub use storage::StrokeLog;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BlendMode, Dab, Quantization, SpaceKind, StrokeHeader, StrokePacket};
    use crate::validation::to_fixed_point;

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
