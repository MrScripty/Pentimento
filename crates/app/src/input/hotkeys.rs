//! Hotkey handling for Pentimento
//!
//! This module handles global hotkeys that aren't forwarded to the webview:
//! - Ctrl+Shift+I: Open DevTools (CEF mode only)
//! - Ctrl+Z: Undo paint stroke
//! - Shift+A: Open add object menu

use bevy::prelude::*;

use super::MouseState;
#[cfg(feature = "cef")]
use crate::config::CompositeMode;
#[cfg(feature = "cef")]
use crate::config::PentimentoConfig;
#[cfg(feature = "cef")]
use crate::render::FrontendResource;

/// Handle Ctrl+Shift+I to open DevTools (CEF mode only)
#[cfg(feature = "cef")]
pub fn handle_devtools_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    config: Res<PentimentoConfig>,
    frontend: Option<NonSend<FrontendResource>>,
) {
    // Only handle in CEF mode
    if config.composite_mode != CompositeMode::Cef {
        return;
    }

    // Check for Ctrl+Shift+I
    let ctrl = key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight);
    let shift = key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);
    let i_pressed = key_input.just_pressed(KeyCode::KeyI);

    if ctrl && shift && i_pressed {
        if let Some(frontend) = frontend {
            info!("Opening CEF DevTools (Ctrl+Shift+I)");
            frontend.backend.show_dev_tools();
        }
    }
}

/// Handle Ctrl+Z for paint undo
pub fn handle_paint_undo_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    mut painting_res: Option<ResMut<pentimento_scene::PaintingResource>>,
) {
    let ctrl = key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight);
    let shift = key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);
    let z_pressed = key_input.just_pressed(KeyCode::KeyZ);

    // Ctrl+Z (without shift) for undo
    if ctrl && !shift && z_pressed {
        if let Some(ref mut painting) = painting_res {
            if painting.undo_any() {
                info!("Paint undo (Ctrl+Z)");
            }
        }
    }
}

/// Handle Shift+A to open the add object menu
pub fn handle_add_menu_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    mouse_state: Res<MouseState>,
    mut outbound: Option<ResMut<pentimento_scene::OutboundUiMessages>>,
) {
    let shift = key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);
    let ctrl = key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight);
    let a_pressed = key_input.just_pressed(KeyCode::KeyA);

    // Shift+A (without ctrl) opens add menu
    if shift && !ctrl && a_pressed {
        if let Some(ref mut outbound) = outbound {
            info!("Opening add object menu (Shift+A)");
            outbound.send(pentimento_ipc::BevyToUi::ShowAddObjectMenu {
                show: true,
                position: Some([mouse_state.webview_x, mouse_state.webview_y]),
            });
        }
    }
}
