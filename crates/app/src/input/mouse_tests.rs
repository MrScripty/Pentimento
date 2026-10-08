use super::*;
use bevy::input::mouse::{MouseButtonInput, MouseWheel};
use bevy::input::{ButtonState, InputPlugin};
use bevy::render::render_resource::TextureFormat;
use bevy::window::CursorMoved;
use pentimento_frontend_core::{CaptureResult, CompositeBackend, FrontendError};
use pentimento_ipc::{BevyToUi, KeyboardEvent, LayoutRegion, UiToBevy};
use pentimento_scene::{FrontendInputBlockState, FrontendUiLayout};
use std::cell::RefCell;
use std::rc::Rc;

use bevy::input::keyboard::{Key, KeyboardInput};

#[derive(Debug)]
enum Recorded {
    Mouse(MouseEvent),
    Keyboard(KeyboardEvent),
}
struct RecordingBackend(Rc<RefCell<Vec<Recorded>>>);
impl CompositeBackend for RecordingBackend {
    fn poll(&mut self) {}
    fn is_ready(&self) -> bool {
        true
    }
    fn capture_if_dirty(&mut self) -> Option<CaptureResult> {
        None
    }
    fn size(&self) -> (u32, u32) {
        (800, 600)
    }
    fn resize(&mut self, _: u32, _: u32) {}
    fn send_mouse_event(&mut self, event: MouseEvent) {
        self.0.borrow_mut().push(Recorded::Mouse(event));
    }
    fn send_keyboard_event(&mut self, event: KeyboardEvent) {
        self.0.borrow_mut().push(Recorded::Keyboard(event));
    }
    fn send_to_ui(&mut self, _: BevyToUi) -> Result<(), FrontendError> {
        Ok(())
    }
    fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        None
    }
}
fn fixture() -> (App, Entity, Rc<RefCell<Vec<Recorded>>>) {
    let mut app = App::new();
    app.add_plugins(InputPlugin)
        .add_message::<WindowEvent>()
        .add_message::<CursorMoved>()
        .init_resource::<super::super::MouseState>()
        .init_resource::<pentimento_config::DisplayConfig>()
        .insert_resource(crate::config::PentimentoConfig {
            composite_mode: crate::config::CompositeMode::Capture,
        })
        .init_resource::<FrontendInputBlockState>()
        .init_resource::<pentimento_scene::PaintMode>()
        .init_resource::<pentimento_scene::ActiveCanvasPlane>()
        .init_resource::<pentimento_scene::OutboundUiMessages>()
        .insert_resource(FrontendUiLayout {
            regions: vec![LayoutRegion {
                id: "brush-panel".into(),
                x: 600.,
                y: 40.,
                width: 200.,
                height: 560.,
                z_index: 1,
                accepts_keyboard: true,
            }],
            received: true,
            pointer_captured: false,
        })
        .add_plugins(super::super::InputPlugin);
    let window = app.world_mut().spawn(Window::default()).id();
    let recorded = Rc::new(RefCell::new(Vec::new()));
    app.insert_non_send_resource(crate::render::FrontendResource {
        backend: Box::new(RecordingBackend(recorded.clone())),
        texture_format: TextureFormat::Rgba8Unorm,
    });
    (app, window, recorded)
}
fn moved(window: Entity, x: f32, y: f32) -> WindowEvent {
    WindowEvent::CursorMoved(CursorMoved {
        window,
        position: Vec2::new(x, y),
        delta: None,
    })
}
fn button(window: Entity, which: bevy::input::mouse::MouseButton, pressed: bool) -> WindowEvent {
    WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: which,
        state: if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    })
}
/// Match winit: publish each typed message and retain the aggregate chronology.
fn native_batch(app: &mut App, events: &[WindowEvent]) {
    for event in events {
        match event.clone() {
            WindowEvent::CursorMoved(event) => {
                app.world_mut()
                    .get_mut::<Window>(event.window)
                    .unwrap()
                    .set_cursor_position(Some(event.position));
                app.world_mut().write_message(event);
            }
            WindowEvent::MouseButtonInput(event) => {
                app.world_mut().write_message(event);
            }
            WindowEvent::MouseWheel(event) => {
                app.world_mut().write_message(event);
            }
            WindowEvent::KeyboardInput(event) => {
                app.world_mut().write_message(event);
            }
            WindowEvent::KeyboardFocusLost(event) => {
                app.world_mut().write_message(event);
            }
            WindowEvent::WindowFocused(event) => {
                app.world_mut()
                    .get_mut::<Window>(event.window)
                    .unwrap()
                    .focused = event.focused;
            }
            _ => {}
        }
        app.world_mut().write_message(event.clone());
    }
    app.update();
}

fn text(window: Entity, value: &str) -> WindowEvent {
    WindowEvent::KeyboardInput(KeyboardInput {
        key_code: KeyCode::KeyA,
        logical_key: Key::Character(value.into()),
        state: ButtonState::Pressed,
        text: Some(value.into()),
        repeat: false,
        window,
    })
}

#[test]
fn rapid_slider_drag_preserves_press_origin_and_blocks_underlying_sculpt() {
    let (mut app, window, recorded) = fixture();
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
            moved(window, 670., 180.),
            moved(window, 400., 250.),
        ],
    );
    assert!(
        app.world()
            .resource::<ButtonInput<bevy::input::mouse::MouseButton>>()
            .just_pressed(bevy::input::mouse::MouseButton::Left)
    );
    assert!(
        app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer(),
        "scene sculpt input must remain blocked after a UI-origin press"
    );
    let trace = recorded.borrow();
    let down = trace.iter().find_map(|event| match event {
        Recorded::Mouse(MouseEvent::ButtonDown { x, y, .. }) => Some((*x, *y)),
        _ => None,
    });
    assert_eq!(
        down,
        Some((700., 180.)),
        "native button coordinates must describe its origin"
    );
}

#[test]
fn complete_button_click_and_viewport_move_keep_the_release_frame_blocked() {
    let (mut app, window, _) = fixture();
    native_batch(
        &mut app,
        &[
            moved(window, 700., 100.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
            button(window, bevy::input::mouse::MouseButton::Left, false),
            moved(window, 300., 250.),
        ],
    );
    assert!(
        app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
    app.update();
    assert!(
        !app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer(),
        "release blocking must not stick on a later idle frame"
    );
}

#[test]
fn slider_button_and_text_field_presses_all_keep_ui_ownership_across_frames() {
    for (name, y) in [("slider", 180.), ("button", 100.), ("text", 240.)] {
        let (mut app, window, recorded) = fixture();
        native_batch(
            &mut app,
            &[
                moved(window, 700., y),
                button(window, bevy::input::mouse::MouseButton::Left, true),
            ],
        );
        native_batch(&mut app, &[moved(window, 400., 250.)]);
        assert!(
            app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer(),
            "{name} lost ownership across the panel boundary"
        );
        native_batch(
            &mut app,
            &[button(window, bevy::input::mouse::MouseButton::Left, false)],
        );
        assert!(
            app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer(),
            "{name} release frame leaked to scene input"
        );
        app.update();
        assert!(
            !app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer()
        );
        assert_eq!(
            recorded
                .borrow()
                .iter()
                .filter(|e| matches!(e, Recorded::Mouse(MouseEvent::ButtonDown { .. })))
                .count(),
            1
        );
    }
}

#[test]
fn earlier_ui_hover_does_not_capture_a_later_viewport_press() {
    let (mut app, window, _) = fixture();
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            moved(window, 400., 250.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
        ],
    );
    assert!(
        !app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
    assert!(!app.world().resource::<FrontendUiLayout>().pointer_captured);
}

#[test]
fn unrelated_button_release_does_not_end_a_ui_owned_drag() {
    let (mut app, window, _) = fixture();
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
        ],
    );
    native_batch(
        &mut app,
        &[
            moved(window, 400., 250.),
            button(window, bevy::input::mouse::MouseButton::Right, true),
            button(window, bevy::input::mouse::MouseButton::Right, false),
        ],
    );
    assert!(
        app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
    assert!(app.world().resource::<FrontendUiLayout>().pointer_captured);
    native_batch(
        &mut app,
        &[button(window, bevy::input::mouse::MouseButton::Left, false)],
    );
    app.update();
    assert!(
        !app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
}

#[test]
fn text_focus_click_and_key_keep_cross_type_order_before_a_later_viewport_click() {
    let (mut app, window, recorded) = fixture();
    native_batch(
        &mut app,
        &[
            moved(window, 700., 240.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
            button(window, bevy::input::mouse::MouseButton::Left, false),
            text(window, "a"),
            moved(window, 400., 250.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
        ],
    );
    let actions: Vec<_> = recorded
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Recorded::Mouse(MouseEvent::ButtonDown { x, y, .. }) => Some(format!("down {x},{y}")),
            Recorded::Mouse(MouseEvent::ButtonUp { x, y, .. }) => Some(format!("up {x},{y}")),
            Recorded::Keyboard(event) => {
                Some(format!("key {}", event.text.as_deref().unwrap_or("")))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        actions,
        ["down 700,240", "up 700,240", "key a", "down 400,250"]
    );
}

#[test]
fn wheel_events_use_their_chronological_positions_and_existing_sign() {
    let (mut app, window, recorded) = fixture();
    let wheel = || {
        WindowEvent::MouseWheel(MouseWheel {
            window,
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: 1.,
            y: 2.,
        })
    };
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            wheel(),
            moved(window, 400., 250.),
            wheel(),
        ],
    );
    let wheels: Vec<_> = recorded
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Recorded::Mouse(MouseEvent::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            }) => Some((*x, *y, *delta_x, *delta_y)),
            _ => None,
        })
        .collect();
    assert_eq!(wheels, [(700., 180., 40., -80.), (400., 250., 40., -80.)]);
}

#[test]
fn coordinates_follow_capture_overlay_and_cef_dpi_contracts() {
    for mode in [
        crate::config::CompositeMode::Capture,
        crate::config::CompositeMode::Overlay,
        crate::config::CompositeMode::Cef,
    ] {
        let (mut app, window, _) = fixture();
        app.world_mut()
            .resource_mut::<crate::config::PentimentoConfig>()
            .composite_mode = mode;
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set_scale_factor_override(Some(2.));
        let (x, y) = if mode == crate::config::CompositeMode::Cef {
            (700., 180.)
        } else {
            (350., 90.)
        };
        native_batch(
            &mut app,
            &[
                moved(window, x, y),
                button(window, bevy::input::mouse::MouseButton::Left, true),
            ],
        );
        let state = app.world().resource::<super::super::MouseState>();
        assert_eq!((state.webview_x, state.webview_y), (700., 180.), "{mode:?}");
        assert!(
            app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer()
        );
    }
}

#[test]
fn browser_producer_preserves_egui_owned_input_flags() {
    let (mut app, window, _) = fixture();
    app.world_mut()
        .resource_mut::<crate::config::PentimentoConfig>()
        .composite_mode = crate::config::CompositeMode::Egui;
    *app.world_mut().resource_mut::<FrontendInputBlockState>() = FrontendInputBlockState {
        block_pointer: true,
        block_keyboard: true,
    };
    native_batch(
        &mut app,
        &[
            moved(window, 400., 250.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
        ],
    );
    let flags = app.world().resource::<FrontendInputBlockState>();
    assert!(flags.block_pointer && flags.block_keyboard);
}

#[test]
fn each_ui_owned_button_releases_independently() {
    let (mut app, window, _) = fixture();
    let left = bevy::input::mouse::MouseButton::Left;
    let right = bevy::input::mouse::MouseButton::Right;
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            button(window, left, true),
            button(window, right, true),
        ],
    );
    native_batch(
        &mut app,
        &[moved(window, 400., 250.), button(window, left, false)],
    );
    assert!(app.world().resource::<FrontendUiLayout>().pointer_captured);
    native_batch(&mut app, &[]);
    assert!(
        app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
    native_batch(&mut app, &[button(window, right, false)]);
    assert!(!app.world().resource::<FrontendUiLayout>().pointer_captured);
    assert!(
        app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
    native_batch(&mut app, &[]);
    assert!(
        !app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
}

#[test]
fn focus_loss_releases_widget_buttons_and_discards_ambiguous_text() {
    let (mut app, window, recorded) = fixture();
    let left = bevy::input::mouse::MouseButton::Left;
    let mut alt_graph = match text(window, "") {
        WindowEvent::KeyboardInput(event) => event,
        _ => unreachable!(),
    };
    alt_graph.key_code = KeyCode::AltRight;
    alt_graph.logical_key = Key::AltGraph;
    alt_graph.text = None;
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            button(window, left, true),
            WindowEvent::KeyboardInput(alt_graph),
        ],
    );
    recorded.borrow_mut().clear();
    native_batch(
        &mut app,
        &[
            moved(window, 400., 250.),
            text(window, "a"),
            WindowEvent::KeyboardFocusLost(bevy::input::keyboard::KeyboardFocusLost),
        ],
    );
    assert!(!app.world().resource::<FrontendUiLayout>().pointer_captured);
    assert!(
        app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
    assert_eq!(
        recorded
            .borrow()
            .iter()
            .filter(|event| matches!(event, Recorded::Mouse(MouseEvent::ButtonUp { .. })))
            .count(),
        1
    );
    assert!(
        !recorded
            .borrow()
            .iter()
            .any(|event| matches!(event, Recorded::Keyboard(_)))
    );
    recorded.borrow_mut().clear();
    native_batch(
        &mut app,
        &[
            WindowEvent::WindowFocused(bevy::window::WindowFocused {
                window,
                focused: true,
            }),
            text(window, "a"),
        ],
    );
    assert!(
        !app.world()
            .resource::<FrontendInputBlockState>()
            .blocks_pointer()
    );
    let trace = recorded.borrow();
    let ordinary = trace
        .iter()
        .find_map(|event| match event {
            Recorded::Keyboard(event) => Some(event),
            _ => None,
        })
        .unwrap();
    assert!(!ordinary.modifiers.alt_graph && !ordinary.modifiers.alt && !ordinary.modifiers.ctrl);
}

#[test]
fn window_focus_loss_also_releases_a_held_pointer() {
    let (mut app, window, recorded) = fixture();
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
        ],
    );
    recorded.borrow_mut().clear();
    native_batch(
        &mut app,
        &[WindowEvent::WindowFocused(bevy::window::WindowFocused {
            window,
            focused: false,
        })],
    );
    assert!(!app.world().resource::<FrontendUiLayout>().pointer_captured);
    assert_eq!(
        recorded
            .borrow()
            .iter()
            .filter(|event| matches!(event, Recorded::Mouse(MouseEvent::ButtonUp { .. })))
            .count(),
        1
    );
}

#[test]
fn browser_waits_for_layout_but_dioxus_and_egui_do_not_wait_for_svelte() {
    for mode in [
        crate::config::CompositeMode::Capture,
        crate::config::CompositeMode::Overlay,
        crate::config::CompositeMode::Cef,
        crate::config::CompositeMode::Dioxus,
        crate::config::CompositeMode::Egui,
    ] {
        let (mut app, window, _) = fixture();
        app.world_mut().resource_mut::<FrontendUiLayout>().received = false;
        app.world_mut()
            .resource_mut::<crate::config::PentimentoConfig>()
            .composite_mode = mode;
        native_batch(&mut app, &[moved(window, 400., 250.)]);
        let waiting = matches!(
            mode,
            crate::config::CompositeMode::Capture
                | crate::config::CompositeMode::Overlay
                | crate::config::CompositeMode::Cef
        );
        assert_eq!(
            app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer(),
            waiting,
            "{mode:?}"
        );
        app.world_mut().resource_mut::<FrontendUiLayout>().received = true;
        native_batch(&mut app, &[]);
        assert!(
            !app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer(),
            "{mode:?}"
        );
    }
}

#[derive(Resource, Default)]
struct TypedCounts {
    moves: usize,
    buttons: usize,
    keys: usize,
}
fn count_typed(
    mut moves: MessageReader<CursorMoved>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut keys: MessageReader<KeyboardInput>,
    mut counts: ResMut<TypedCounts>,
) {
    counts.moves += moves.read().count();
    counts.buttons += buttons.read().count();
    counts.keys += keys.read().count();
}
#[test]
fn ordered_forwarding_leaves_typed_messages_available_and_does_not_duplicate_packets() {
    let (mut app, window, recorded) = fixture();
    app.init_resource::<TypedCounts>()
        .add_systems(PostUpdate, count_typed);
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
            button(window, bevy::input::mouse::MouseButton::Left, false),
            text(window, "a"),
        ],
    );
    native_batch(&mut app, &[]);
    let counts = app.world().resource::<TypedCounts>();
    assert_eq!((counts.moves, counts.buttons, counts.keys), (1, 2, 1));
    let trace = recorded.borrow();
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event, Recorded::Mouse(MouseEvent::ButtonDown { .. })))
            .count(),
        1
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event, Recorded::Mouse(MouseEvent::ButtonUp { .. })))
            .count(),
        1
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event, Recorded::Keyboard(_)))
            .count(),
        1
    );
}

#[test]
fn viewport_focus_loss_blocks_held_scene_input_until_window_focus_returns() {
    for keyboard_loss in [false, true] {
        let (mut app, window, _) = fixture();
        let left = bevy::input::mouse::MouseButton::Left;
        native_batch(
            &mut app,
            &[moved(window, 400., 250.), button(window, left, true)],
        );
        assert!(
            !app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer()
        );
        let loss = if keyboard_loss {
            WindowEvent::KeyboardFocusLost(bevy::input::keyboard::KeyboardFocusLost)
        } else {
            WindowEvent::WindowFocused(bevy::window::WindowFocused {
                window,
                focused: false,
            })
        };
        native_batch(&mut app, &[loss]);
        // Bevy retains mouse held state: scene must be blocked independently.
        assert!(
            app.world()
                .resource::<ButtonInput<bevy::input::mouse::MouseButton>>()
                .pressed(left)
        );
        assert!(
            app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer()
        );
        native_batch(&mut app, &[]);
        assert!(
            app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer()
        );
        native_batch(
            &mut app,
            &[
                WindowEvent::WindowFocused(bevy::window::WindowFocused {
                    window,
                    focused: true,
                }),
                button(window, left, false),
            ],
        );
        assert!(
            !app.world()
                .resource::<FrontendInputBlockState>()
                .blocks_pointer()
        );
    }
}

// Full production Bevy reducer -> native forwarding plugin -> paint mode plugin
// -> painting plugin. The recording UI backend observes forwarding only; it
// neither fabricates PaintEvents nor replaces the CPU painting pipeline.
fn native_paint_fixture() -> (App, Entity) {
    use bevy::camera::{ComputedCameraValues, RenderTargetInfo};
    use pentimento_scene::{
        ActiveCanvasPlane, CanvasPlane, MainCamera, PaintModePlugin, PaintingSystemPlugin,
    };
    let (mut app, window, _) = fixture();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<pentimento_scene::EditModeState>()
        .add_plugins((PaintModePlugin, PaintingSystemPlugin));
    app.world_mut()
        .entity_mut(window)
        .insert(bevy::window::PrimaryWindow);
    app.world_mut().spawn((
        Camera {
            computed: ComputedCameraValues {
                clip_from_view: Mat4::perspective_infinite_reverse_rh(
                    std::f32::consts::FRAC_PI_4,
                    1.,
                    0.1,
                ),
                target_info: Some(RenderTargetInfo {
                    physical_size: UVec2::splat(1000),
                    scale_factor: 1.,
                }),
                ..default()
            },
            ..default()
        },
        GlobalTransform::from(Transform::from_xyz(0., 0., 4.).looking_at(Vec3::ZERO, Vec3::Y)),
        MainCamera,
    ));
    let material = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::default());
    let plane = app
        .world_mut()
        .spawn((
            CanvasPlane::new(7, 128, 128, 4., 4.),
            GlobalTransform::IDENTITY,
            MeshMaterial3d(material),
        ))
        .id();
    app.world_mut().resource_mut::<ActiveCanvasPlane>().entity = Some(plane);
    app.world_mut()
        .resource_mut::<pentimento_scene::PaintMode>()
        .active = true;
    app.update();
    (app, window)
}
fn native_paint_gesture(window: Entity) -> Vec<WindowEvent> {
    vec![
        moved(window, 300., 300.),
        button(window, bevy::input::mouse::MouseButton::Left, true),
        moved(window, 350., 310.),
        moved(window, 400., 320.),
        moved(window, 450., 330.),
        button(window, bevy::input::mouse::MouseButton::Left, false),
    ]
}
fn native_paint_pixels(app: &App) -> Vec<u8> {
    app.world()
        .resource::<pentimento_scene::PaintingResource>()
        .get_pipeline(7)
        .unwrap()
        .surface_as_bytes()
        .to_vec()
}
fn assert_native_paint_complete(app: &App, count: usize) {
    let p = app
        .world()
        .resource::<pentimento_scene::PaintingResource>()
        .get_pipeline(7)
        .unwrap();
    assert_eq!(p.undo_count(), count, "actual undo entries");
    assert_eq!(
        p.log().total_packet_count(),
        count,
        "actual completed packets"
    );
    assert!(!p.is_stroking(), "pipeline must release ownership");
    assert!(
        app.world()
            .resource::<pentimento_scene::PaintMode>()
            .current_stroke
            .is_none()
    );
}
fn native_paint_control() -> Vec<u8> {
    let (mut app, window) = native_paint_fixture();
    let before = native_paint_pixels(&app);
    for event in native_paint_gesture(window) {
        native_batch(&mut app, &[event]);
    }
    assert_native_paint_complete(&app, 1);
    let after = native_paint_pixels(&app);
    assert_ne!(after, before);
    after
}
#[test]
fn native_paint_separate_frame_positive_control() {
    native_paint_control();
}

#[test]
fn native_paint_stationary_press_after_ui_closes_uses_current_origin() {
    let (mut control, control_window) = native_paint_fixture();
    control
        .world_mut()
        .resource_mut::<FrontendUiLayout>()
        .regions
        .clear();
    native_batch(
        &mut control,
        &[
            moved(control_window, 700., 180.),
            button(control_window, bevy::input::mouse::MouseButton::Left, true),
            moved(control_window, 701., 181.),
            button(control_window, bevy::input::mouse::MouseButton::Left, false),
        ],
    );
    assert_native_paint_complete(&control, 1);
    let expected = native_paint_pixels(&control);

    let (mut app, window) = native_paint_fixture();
    let before = native_paint_pixels(&app);
    native_batch(&mut app, &[moved(window, 350., 320.)]);
    native_batch(&mut app, &[moved(window, 700., 180.)]);
    assert!(
        native_paint_pixels(&app) == before,
        "UI hover must not paint"
    );
    app.world_mut()
        .resource_mut::<FrontendUiLayout>()
        .regions
        .clear();
    native_batch(&mut app, &[]);
    // Native press has no new cursor event: the UI disappeared under the pointer.
    native_batch(
        &mut app,
        &[
            button(window, bevy::input::mouse::MouseButton::Left, true),
            moved(window, 701., 181.),
            button(window, bevy::input::mouse::MouseButton::Left, false),
        ],
    );
    assert_native_paint_complete(&app, 1);
    assert!(
        native_paint_pixels(&app) == expected,
        "stationary press must match fresh current-position stroke, without a line from stale scene hover"
    );
    assert!(
        app.world_mut()
            .resource_mut::<pentimento_scene::PaintingResource>()
            .get_pipeline_mut(7)
            .unwrap()
            .undo()
    );
    app.update();
    assert!(
        native_paint_pixels(&app) == before,
        "undo restores the entire accepted stroke"
    );
}

#[test]
fn native_paint_fine_moves_cover_the_continuous_path_and_undo_exactly() {
    let (mut app, window) = native_paint_fixture();
    {
        let mut paint = app
            .world_mut()
            .resource_mut::<pentimento_scene::PaintingResource>();
        paint.set_brush_color([1., 0., 1., 1.]);
        let mut preset = paint.brush_preset.clone();
        preset.base_size = 8.;
        preset.min_size = 8.;
        preset.max_size = 8.;
        preset.hardness = 1.;
        preset.spacing = 0.25;
        paint.set_brush_preset(preset);
    }
    let before = native_paint_pixels(&app);
    let mut events = vec![
        moved(window, 300., 500.),
        button(window, bevy::input::mouse::MouseButton::Left, true),
    ];
    // Each three-pixel native movement is below half a dab spacing after the
    // real fixture camera projects it into this 128px canvas.
    for x in (303..=570).step_by(3) {
        events.push(moved(window, x as f32, 500.));
    }
    events.push(button(window, bevy::input::mouse::MouseButton::Left, false));
    native_batch(&mut app, &events);
    assert_native_paint_complete(&app, 1);
    // Locate the screen path through the real fixture camera. The canvas is the
    // four-unit identity XY rectangle used by native_paint_fixture.
    let world = app.world_mut();
    let mut cameras =
        world.query_filtered::<(&Camera, &GlobalTransform), With<pentimento_scene::MainCamera>>();
    let (camera, transform) = cameras.single(world).unwrap();
    for x in (300..=570).step_by(3) {
        let ray = camera
            .viewport_to_world(transform, Vec2::new(x as f32, 500.))
            .unwrap();
        let point = ray.origin + ray.direction * (-ray.origin.z / ray.direction.z);
        let px = ((point.x / 4. + 0.5) * 128.).floor() as usize;
        let py = ((-point.y / 4. + 0.5) * 128.).floor() as usize;
        let pixel = world
            .resource::<pentimento_scene::PaintingResource>()
            .get_pipeline(7)
            .unwrap()
            .get_pixel(px as u32, py as u32)
            .unwrap();
        assert_eq!(
            pixel,
            [1., 0., 1., 1.],
            "unpainted path at screen {x}, canvas ({px},{py})"
        );
    }
    assert!(
        app.world_mut()
            .resource_mut::<pentimento_scene::PaintingResource>()
            .get_pipeline_mut(7)
            .unwrap()
            .undo()
    );
    app.update();
    assert!(
        native_paint_pixels(&app) == before,
        "undo must restore the complete fine-movement stroke"
    );
}
#[test]
fn native_paint_complete_gesture_then_focus_loss_preserves_valid_prefix() {
    let expected = native_paint_control();
    let (mut app, window) = native_paint_fixture();
    let mut events = native_paint_gesture(window);
    events.push(WindowEvent::WindowFocused(bevy::window::WindowFocused {
        window,
        focused: false,
    }));
    native_batch(&mut app, &events);
    assert_native_paint_complete(&app, 1);
    assert!(
        native_paint_pixels(&app) == expected,
        "pixels must match real separate-frame control"
    );
}
#[test]
fn native_paint_complete_gesture_then_ui_hover_preserves_valid_prefix() {
    let expected = native_paint_control();
    let (mut app, window) = native_paint_fixture();
    let mut events = native_paint_gesture(window);
    events.push(moved(window, 700., 180.));
    native_batch(&mut app, &events);
    assert_native_paint_complete(&app, 1);
    assert!(
        native_paint_pixels(&app) == expected,
        "pixels must match real separate-frame control"
    );
}
#[test]
fn native_paint_ui_click_then_complete_gesture_preserves_valid_suffix() {
    let expected = native_paint_control();
    let (mut app, window) = native_paint_fixture();
    let mut events = vec![
        moved(window, 700., 180.),
        button(window, bevy::input::mouse::MouseButton::Left, true),
        button(window, bevy::input::mouse::MouseButton::Left, false),
    ];
    events.extend(native_paint_gesture(window));
    native_batch(&mut app, &events);
    assert_native_paint_complete(&app, 1);
    assert!(
        native_paint_pixels(&app) == expected,
        "pixels must match real separate-frame control"
    );
}
fn escape(window: Entity) -> WindowEvent {
    WindowEvent::KeyboardInput(KeyboardInput {
        window,
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
    })
}
#[test]
fn native_paint_same_frame_escape_cancels_the_chronological_gesture() {
    let (mut app, window) = native_paint_fixture();
    let before = native_paint_pixels(&app);
    let mut events = native_paint_gesture(window);
    events.insert(events.len() - 1, escape(window));
    native_batch(&mut app, &events);
    assert_native_paint_complete(&app, 0);
    assert!(
        native_paint_pixels(&app) == before,
        "Escape must restore painted tiles"
    );
}
#[test]
fn native_paint_prior_stroke_end_then_escape_cancels_only_the_new_gesture() {
    let expected = native_paint_control();
    let (mut app, window) = native_paint_fixture();
    let mut first = native_paint_gesture(window);
    let release = first.pop().unwrap();
    native_batch(&mut app, &first);
    let mut events = vec![release];
    let mut second = native_paint_gesture(window);
    second.insert(second.len() - 1, escape(window));
    events.extend(second);
    native_batch(&mut app, &events);
    assert_native_paint_complete(&app, 1);
    assert!(
        native_paint_pixels(&app) == expected,
        "pixels must match real separate-frame control"
    );
}
#[test]
fn native_paint_ui_drag_cannot_paint_after_leaving_panel_or_replay_later() {
    let (mut app, window) = native_paint_fixture();
    let before = native_paint_pixels(&app);
    native_batch(
        &mut app,
        &[
            moved(window, 700., 180.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
            moved(window, 350., 310.),
            moved(window, 400., 320.),
            button(window, bevy::input::mouse::MouseButton::Left, false),
        ],
    );
    for _ in 0..3 {
        app.update();
    }
    assert_native_paint_complete(&app, 0);
    assert!(
        native_paint_pixels(&app) == before,
        "UI input must leave pixels unchanged"
    );
}

#[test]
fn native_paint_stationary_stroke_closes_when_new_ui_layout_covers_cursor() {
    let (mut app, window) = native_paint_fixture();
    native_batch(
        &mut app,
        &[
            moved(window, 350., 310.),
            button(window, bevy::input::mouse::MouseButton::Left, true),
        ],
    );
    assert!(
        app.world()
            .resource::<pentimento_scene::PaintMode>()
            .current_stroke
            .is_some()
    );
    app.world_mut()
        .resource_mut::<FrontendUiLayout>()
        .regions
        .push(LayoutRegion {
            id: "new-menu".into(),
            x: 300.,
            y: 280.,
            width: 100.,
            height: 100.,
            z_index: 2,
            accepts_keyboard: true,
        });
    // No motion or button event accompanies this actual layout update.
    native_batch(&mut app, &[]);
    assert_native_paint_complete(&app, 1);
    let closed = native_paint_pixels(&app);
    for _ in 0..3 {
        native_batch(&mut app, &[]);
    }
    assert!(
        native_paint_pixels(&app) == closed,
        "covered stationary cursor must not keep painting"
    );
    // Leaving the menu while still physically held cannot resume this stroke.
    native_batch(&mut app, &[moved(window, 450., 330.)]);
    assert_native_paint_complete(&app, 1);
    assert!(native_paint_pixels(&app) == closed);
    native_batch(
        &mut app,
        &[button(window, bevy::input::mouse::MouseButton::Left, false)],
    );
    assert_native_paint_complete(&app, 1);
}
