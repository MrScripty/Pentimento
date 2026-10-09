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

/// Native forwarding publishes only admitted shortcuts for this frame.
/// Keep actions rather than a key count: focus return can admit a suffix after
/// earlier rejected keys, and modifier releases may follow a UI focus click.
#[derive(Resource, Default)]
pub(super) struct EguiPaintHistory {
    pub(super) actions: Vec<PaintHistoryAction>,
}

#[derive(Clone, Copy)]
pub(super) enum PaintHistoryAction {
    Undo,
    Redo,
}

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
    config: Option<Res<crate::config::PentimentoConfig>>,
    admitted: Option<Res<EguiPaintHistory>>,
    scene_history_owner: Option<Res<pentimento_scene::NativeSceneHistoryOwner>>,
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
    if input_blocks.blocks_keyboard()
        || lost
        || !paint_mode.active
        || paint_mode.target == pentimento_ipc::PaintTarget::DirectUv
    {
        return;
    }

    if config
        .as_ref()
        .is_some_and(|c| c.composite_mode == crate::config::CompositeMode::Egui)
        && scene_history_owner.is_some()
    {
        return;
    }
    let actions = if config
        .as_ref()
        .is_some_and(|c| c.composite_mode == crate::config::CompositeMode::Egui)
    {
        admitted.map_or_else(Vec::new, |input| input.actions.clone())
    } else {
        // Other frontends retain their full-batch chronological modifier route.
        let translated = super::keyboard::translate_keyboard_events(
            &events,
            &key_input,
            &mut ButtonInput::default(),
            false,
        );
        events
            .iter()
            .zip(translated)
            .filter_map(|(native, event)| {
                (native.key_code == KeyCode::KeyZ
                    && event.pressed
                    && !native.repeat
                    && event.modifiers.ctrl)
                    .then_some(if event.modifiers.shift {
                        PaintHistoryAction::Redo
                    } else {
                        PaintHistoryAction::Undo
                    })
            })
            .collect()
    };
    for action in actions {
        let shift = matches!(action, PaintHistoryAction::Redo);
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

#[cfg(test)]
mod direct_history_routing_tests {
    use super::*;
    use bevy::input::{
        ButtonState,
        keyboard::{Key, KeyboardFocusLost, KeyboardInput},
    };
    use bevy::window::WindowEvent;
    use pentimento_scene::{ActiveCanvasPlane, CanvasPlane, PaintMode, PaintingResource};

    #[test]
    fn direct_shortcut_cannot_also_undo_canvas_or_replay_after_a_mode_switch() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<FrontendInputBlockState>()
            .init_resource::<PaintingResource>()
            .add_message::<KeyboardInput>()
            .add_message::<KeyboardFocusLost>()
            .add_message::<WindowEvent>()
            .add_systems(Update, handle_paint_undo_hotkey);
        let canvas = app
            .world_mut()
            .spawn(CanvasPlane::new(7, 32, 32, 2., 2.))
            .id();
        app.insert_resource(ActiveCanvasPlane {
            entity: Some(canvas),
            camera_locked: false,
        });
        app.insert_resource(PaintMode {
            active: true,
            target: pentimento_ipc::PaintTarget::DirectUv,
            ..default()
        });
        let mut painting = app.world_mut().resource_mut::<PaintingResource>();
        let pipeline = painting.get_or_create_pipeline(7, 32, 32);
        pipeline.begin_stroke(7, 1, 0);
        pipeline.stroke_to(16., 16., 1.);
        pipeline.end_stroke();
        assert_eq!(pipeline.undo_count(), 1);
        let input = KeyboardInput {
            key_code: KeyCode::KeyZ,
            logical_key: Key::Character("z".into()),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        };
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        app.world_mut().write_message(input.clone());
        app.update();
        assert_eq!(
            app.world()
                .resource::<PaintingResource>()
                .get_pipeline(7)
                .unwrap()
                .undo_count(),
            1
        );
        app.world_mut().resource_mut::<PaintMode>().target = pentimento_ipc::PaintTarget::Canvas;
        app.update();
        assert_eq!(
            app.world()
                .resource::<PaintingResource>()
                .get_pipeline(7)
                .unwrap()
                .undo_count(),
            1
        );
        app.world_mut().write_message(input);
        app.update();
        assert_eq!(
            app.world()
                .resource::<PaintingResource>()
                .get_pipeline(7)
                .unwrap()
                .undo_count(),
            0
        );
    }
}
