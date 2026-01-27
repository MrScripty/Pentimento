//! Thread-safe append-only storage for stroke packets.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::types::StrokePacket;

use super::events::StrokeLogEvent;

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
