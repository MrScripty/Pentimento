//! Webview lifecycle state management

/// Webview lifecycle states for managing capture timing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebviewState {
    /// Just created, waiting for content to load
    Initializing,
    /// Content loaded, warming up for first capture
    WarmingUp { frames_remaining: u32 },
    /// Ready for normal capture operations
    Ready,
    /// Resize in progress, waiting for stabilization
    Resizing { frames_remaining: u32 },
}

/// Number of frames to wait during warmup before first capture (~1 second at 60fps)
pub const WARMUP_FRAMES: u32 = 60;

/// Number of GTK iterations per poll during warmup/initialization
pub const WARMUP_GTK_ITERATIONS: u32 = 20;

/// Number of frames to wait after resize before capture (increased for WebKit to process)
pub const RESIZE_DEBOUNCE_FRAMES: u32 = 30;

/// Number of GTK iterations per poll in Ready state
/// Must be sufficient for WebKit to process layout/paint operations
pub const READY_GTK_ITERATIONS: u32 = 30;

/// Number of frames to wait after mouse event before allowing capture
/// This allows RAF callbacks and WebKit layout/paint to complete
pub const MOUSE_EVENT_SETTLE_FRAMES: u32 = 3;
