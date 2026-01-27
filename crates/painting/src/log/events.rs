//! Events emitted during stroke recording for Iroh integration hooks.

use crate::types::StrokePacket;

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
