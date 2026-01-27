//! Offscreen rendering capture (BGRA format)
//!
//! This module provides utilities for capturing the CEF offscreen framebuffer.
//! CEF renders to BGRA format, which can be used directly with zero-copy Arc sharing.

use crate::browser::SharedState;
use pentimento_frontend_core::CaptureResult;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Capture the current framebuffer if it has changed
///
/// Returns `Some(CaptureResult::Bgra)` if the framebuffer has been updated,
/// or `None` if the content hasn't changed since the last capture.
///
/// The returned Arc allows zero-copy sharing - cloning is just a pointer copy (~20ns)
/// vs copying the entire buffer (~6-12ms for 18MB at HiDPI).
pub fn capture_if_dirty(shared: &Arc<SharedState>) -> Option<CaptureResult> {
    // Check and clear the dirty flag atomically
    if !shared.dirty.swap(false, Ordering::SeqCst) {
        return None;
    }

    // Arc clone is instant (~20ns) vs Vec clone (~6-12ms for 18MB)
    let buffer = shared.framebuffer.lock().unwrap().clone()?;
    let (width, height) = *shared.framebuffer_size.lock().unwrap();

    Some(CaptureResult::Bgra(buffer, width, height))
}

/// Capture the current framebuffer unconditionally
///
/// Returns the current framebuffer regardless of dirty state.
/// Useful for debugging or when you need the current state.
pub fn capture_unconditional(shared: &Arc<SharedState>) -> Option<(Arc<Vec<u8>>, u32, u32)> {
    let buffer = shared.framebuffer.lock().unwrap().clone()?;
    let (width, height) = *shared.framebuffer_size.lock().unwrap();
    Some((buffer, width, height))
}

/// Check if a framebuffer is available
pub fn has_framebuffer(shared: &Arc<SharedState>) -> bool {
    shared.framebuffer.lock().unwrap().is_some()
}

/// Get the current framebuffer dimensions
pub fn framebuffer_size(shared: &Arc<SharedState>) -> (u32, u32) {
    *shared.framebuffer_size.lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    fn create_test_shared_state() -> Arc<SharedState> {
        let (tx, _rx) = mpsc::unbounded_channel();
        Arc::new(SharedState {
            framebuffer: Mutex::new(None),
            framebuffer_size: Mutex::new((0, 0)),
            dirty: Arc::new(AtomicBool::new(false)),
            size: Mutex::new((800, 600)),
            from_ui_tx: tx,
        })
    }

    #[test]
    fn test_capture_if_dirty_returns_none_when_not_dirty() {
        let shared = create_test_shared_state();
        assert!(capture_if_dirty(&shared).is_none());
    }

    #[test]
    fn test_capture_if_dirty_returns_none_when_dirty_but_no_buffer() {
        let shared = create_test_shared_state();
        shared.dirty.store(true, Ordering::SeqCst);
        assert!(capture_if_dirty(&shared).is_none());
    }

    #[test]
    fn test_capture_if_dirty_returns_buffer_when_dirty_and_available() {
        let shared = create_test_shared_state();
        let test_buffer = vec![0u8; 800 * 600 * 4];
        *shared.framebuffer.lock().unwrap() = Some(Arc::new(test_buffer));
        *shared.framebuffer_size.lock().unwrap() = (800, 600);
        shared.dirty.store(true, Ordering::SeqCst);

        let result = capture_if_dirty(&shared);
        assert!(result.is_some());

        match result.unwrap() {
            CaptureResult::Bgra(buffer, width, height) => {
                assert_eq!(width, 800);
                assert_eq!(height, 600);
                assert_eq!(buffer.len(), 800 * 600 * 4);
            }
            _ => panic!("Expected Bgra result"),
        }

        // Dirty flag should be cleared
        assert!(!shared.dirty.load(Ordering::SeqCst));
    }

    #[test]
    fn test_has_framebuffer() {
        let shared = create_test_shared_state();
        assert!(!has_framebuffer(&shared));

        *shared.framebuffer.lock().unwrap() = Some(Arc::new(vec![0u8; 4]));
        assert!(has_framebuffer(&shared));
    }
}
