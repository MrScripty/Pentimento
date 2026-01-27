//! IPC bridge between Dioxus components and Bevy
//!
//! Re-exports and wraps the bridge types from pentimento-dioxus-ui,
//! providing a unified interface for the frontend-dioxus crate.

pub use pentimento_dioxus_ui::{DioxusBridge as DioxusBridgeInner, DioxusBridgeHandle};

use pentimento_ipc::{BevyToUi, UiToBevy};

/// Wrapper around the Dioxus UI bridge for frontend integration.
///
/// This provides a higher-level interface for managing communication
/// between the Bevy application and the Dioxus UI components.
pub struct DioxusBridge {
    /// The inner bridge (Dioxus component side)
    inner: DioxusBridgeInner,
    /// The handle (Bevy side)
    handle: Option<DioxusBridgeHandle>,
}

impl DioxusBridge {
    /// Create a new bridge pair.
    ///
    /// Returns the bridge wrapper. Use `take_handle()` to get the
    /// `DioxusBridgeHandle` for passing to the `DioxusBackend`.
    pub fn new() -> Self {
        let (inner, handle) = DioxusBridgeInner::new();
        Self {
            inner,
            handle: Some(handle),
        }
    }

    /// Take the bridge handle for use with `DioxusBackend`.
    ///
    /// This can only be called once; subsequent calls return `None`.
    pub fn take_handle(&mut self) -> Option<DioxusBridgeHandle> {
        self.handle.take()
    }

    /// Get a reference to the inner Dioxus bridge.
    ///
    /// This is passed to Dioxus components as props.
    pub fn inner(&self) -> &DioxusBridgeInner {
        &self.inner
    }

    /// Clone the inner bridge for passing to Dioxus components.
    pub fn clone_inner(&self) -> DioxusBridgeInner {
        self.inner.clone()
    }

    /// Check if there are pending messages from Bevy.
    pub fn has_pending_messages(&self) -> bool {
        self.inner.has_pending_messages()
    }

    /// Clear the pending messages flag.
    pub fn clear_pending(&self) {
        self.inner.clear_pending();
    }

    /// Try to receive a message from Bevy (non-blocking).
    pub fn try_recv_from_bevy(&self) -> Option<BevyToUi> {
        self.inner.try_recv_from_bevy()
    }

    /// Send a UI dirty notification.
    pub fn mark_dirty(&self) {
        self.inner.mark_dirty();
    }
}

impl Default for DioxusBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for `DioxusBridgeHandle` to provide additional functionality.
pub trait DioxusBridgeHandleExt {
    /// Send a message to the UI, logging any errors.
    fn send_logged(&self, msg: BevyToUi);

    /// Try to receive all pending messages from the UI.
    fn drain_messages(&self) -> Vec<UiToBevy>;
}

impl DioxusBridgeHandleExt for DioxusBridgeHandle {
    fn send_logged(&self, msg: BevyToUi) {
        self.send(msg);
    }

    fn drain_messages(&self) -> Vec<UiToBevy> {
        let mut messages = Vec::new();
        while let Some(msg) = self.try_recv() {
            messages.push(msg);
        }
        messages
    }
}
