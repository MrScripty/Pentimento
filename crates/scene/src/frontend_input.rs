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
