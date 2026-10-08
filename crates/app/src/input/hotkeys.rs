//! Hotkey handling for Pentimento
//!
//! This module handles global hotkeys that aren't forwarded to the webview:
//! - Ctrl+Shift+I: Open DevTools (CEF mode only)
//! - Ctrl+Z / Ctrl+Shift+Z: Undo / Redo paint stroke
//! - Shift+A: Open add object menu

use bevy::prelude::*;
use pentimento_scene::FrontendInputBlockState;

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

/// Handle Ctrl+Z / Ctrl+Shift+Z for the active canvas history
pub fn handle_paint_undo_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut focus_lost: MessageReader<bevy::input::keyboard::KeyboardFocusLost>,
    mut window_events: MessageReader<bevy::window::WindowEvent>,
    input_blocks: Res<FrontendInputBlockState>,
    mut painting_res: Option<ResMut<pentimento_scene::PaintingResource>>,
    paint_mode: Res<pentimento_scene::PaintMode>,
    active_canvas: Res<pentimento_scene::ActiveCanvasPlane>,
    canvases: Query<&pentimento_scene::CanvasPlane>,
) {
    let events: Vec<_> = keys.read().cloned().collect();
    let keyboard_lost = focus_lost.read().count() > 0;
    let window_lost = window_events.read().fold(false, |lost, event| {
        lost || matches!(event,
            bevy::window::WindowEvent::WindowFocused(event) if !event.focused)
    });
    let lost = keyboard_lost || window_lost;
    if input_blocks.blocks_keyboard() || lost || !paint_mode.active {
        return;
    }

    // A full shortcut can arrive within one frame. Restore modifiers at each
    // native event instead of using the final (possibly released) state.
    let translated = super::keyboard::translate_keyboard_events(
        &events,
        &key_input,
        &mut ButtonInput::default(),
        false,
    );
    for (native, event) in events.iter().zip(translated) {
        if native.key_code != KeyCode::KeyZ
            || !event.pressed
            || native.repeat
            || !event.modifiers.ctrl
        {
            continue;
        }
        let shift = event.modifiers.shift;
        if let Some(ref mut painting) = painting_res {
            if let Some(canvas) = active_canvas
                .entity
                .and_then(|entity| canvases.get(entity).ok())
            {
                if if shift {
                    painting.redo(canvas.plane_id)
                } else {
                    painting.undo(canvas.plane_id)
                } {
                    info!("Paint history restored (redo={})", shift);
                }
            }
        }
    }
}

/// Handle Shift+A to open the add object menu
pub fn handle_add_menu_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    mouse_state: Res<MouseState>,
    input_blocks: Res<FrontendInputBlockState>,
    mut outbound: Option<ResMut<pentimento_scene::OutboundUiMessages>>,
) {
    if input_blocks.blocks_keyboard() {
        return;
    }

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
