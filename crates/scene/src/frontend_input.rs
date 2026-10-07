//! Frontend-owned input arbitration state.
//!
//! Native frontends can use this resource to prevent viewport systems from
//! reacting while UI widgets own pointer or keyboard focus.

use bevy::prelude::*;

/// Generic input blocking flags owned by the active frontend integration.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct FrontendInputBlockState {
    /// Prevent viewport and scene systems from consuming pointer input.
    pub block_pointer: bool,
    /// Prevent viewport and scene systems from consuming keyboard input.
    pub block_keyboard: bool,
}

impl FrontendInputBlockState {
    /// Returns true when scene systems should ignore pointer input.
    pub fn blocks_pointer(self) -> bool {
        self.block_pointer
    }

    /// Returns true when scene systems should ignore keyboard input.
    pub fn blocks_keyboard(self) -> bool {
        self.block_keyboard
    }
}

/// Browser-owned rectangles in webview coordinates; native and WASM hosts can
/// block viewport input before forwarding a click into an interactive widget.
#[derive(Resource, Default)]
pub struct FrontendUiLayout {
    pub regions: Vec<pentimento_ipc::LayoutRegion>,
    pub received: bool,
    pub pointer_captured: bool,
}

impl FrontendUiLayout {
    pub fn update_pointer(&mut self, x: f32, y: f32, down: bool, just_pressed: bool) -> bool {
        let over_ui = self
            .regions
            .iter()
            .any(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height);
        if just_pressed && over_ui {
            self.pointer_captured = true;
        }
        let blocked = over_ui || self.pointer_captured;
        if !down {
            self.pointer_captured = false;
        }
        blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widget_drag_stays_captured_until_release() {
        let mut layout = FrontendUiLayout {
            regions: vec![pentimento_ipc::LayoutRegion {
                id: "brush".into(),
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
                z_index: 1,
                accepts_keyboard: true,
            }],
            received: true,
            ..default()
        };
        assert!(layout.update_pointer(15.0, 15.0, true, true));
        assert!(layout.update_pointer(200.0, 200.0, true, false));
        assert!(layout.update_pointer(200.0, 200.0, false, false));
        assert!(!layout.update_pointer(200.0, 200.0, false, false));
    }
}
