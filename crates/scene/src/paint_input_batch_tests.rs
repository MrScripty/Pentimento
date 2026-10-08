//! Drives real Bevy mouse reduction and the production PaintingSystemPlugin.
//! Never synthesizes PaintEvent or manipulates ButtonInput directly.
use super::*;
use crate::{
    camera::MainCamera,
    frontend_input::FrontendInputBlockState,
    paint_mode::{PaintMode, StrokeIdGenerator, handle_paint_input},
};
use bevy::camera::{ComputedCameraValues, RenderTargetInfo};
use bevy::input::{
    ButtonState, InputPlugin,
    mouse::{MouseButton, MouseButtonInput},
};
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent};

fn fixture() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin))
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<crate::OutboundUiMessages>()
        .init_resource::<ActiveCanvasPlane>()
        .init_resource::<FrontendInputBlockState>()
        .init_resource::<StrokeIdGenerator>()
        .insert_resource(PaintMode {
            active: true,
            current_stroke: None,
        })
        .add_message::<CursorMoved>()
        .add_message::<WindowEvent>()
        .add_message::<PaintEvent>()
        .add_plugins(PaintingSystemPlugin)
        .add_systems(Update, handle_paint_input.before(process_paint_events));
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
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
    let mut paint = app.world_mut().resource_mut::<PaintingResource>();
    paint.brush_color = [1., 0., 0., 1.];
    paint.brush_preset = BrushPreset {
        base_size: 8.,
        min_size: 8.,
        max_size: 8.,
        ..default()
    };
    drop(paint);
    app.update();
    (app, window)
}

fn movement(window: Entity, x: f32) -> WindowEvent {
    WindowEvent::CursorMoved(CursorMoved {
        window,
        position: Vec2::new(x, 500.),
        delta: None,
    })
}
fn button(window: Entity, down: bool) -> WindowEvent {
    WindowEvent::MouseButtonInput(MouseButtonInput {
        window,
        button: MouseButton::Left,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
    })
}
fn gesture(window: Entity) -> Vec<WindowEvent> {
    vec![
        movement(window, 350.),
        button(window, true),
        movement(window, 400.),
        movement(window, 450.),
        movement(window, 500.),
        movement(window, 550.),
        movement(window, 600.),
        movement(window, 650.),
        button(window, false),
    ]
}
fn batch(app: &mut App, events: Vec<WindowEvent>) {
    // The host emits both ordered and typed messages, exactly as Bevy/winit does.
    for event in events {
        match &event {
            WindowEvent::CursorMoved(e) => {
                app.world_mut()
                    .get_mut::<Window>(e.window)
                    .unwrap()
                    .set_cursor_position(Some(e.position));
                app.world_mut().write_message(e.clone());
            }
            WindowEvent::MouseButtonInput(e) => {
                app.world_mut().write_message(*e);
            }
            WindowEvent::WindowFocused(e) => {
                app.world_mut().get_mut::<Window>(e.window).unwrap().focused = e.focused;
            }
            _ => unreachable!(),
        }
        app.world_mut().write_message(event);
    }
    app.update();
}
fn pixels(app: &App) -> Vec<u8> {
    app.world()
        .resource::<PaintingResource>()
        .get_pipeline(7)
        .unwrap()
        .surface_as_bytes()
        .to_vec()
}
fn completed(app: &App, count: usize) {
    let pipeline = app
        .world()
        .resource::<PaintingResource>()
        .get_pipeline(7)
        .unwrap();
    assert_eq!(
        pipeline.log().total_packet_count(),
        count,
        "real completion packets"
    );
    assert_eq!(pipeline.undo_count(), count, "real undo entries");
    assert!(!pipeline.is_stroking(), "pipeline ownership released");
    assert!(
        app.world().resource::<PaintMode>().current_stroke.is_none(),
        "input ownership released"
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
    for (id, packet) in pipeline.log().query_by_space(7).iter().enumerate() {
        assert_eq!(packet.header.stroke_id, id as u64);
        assert!(
            packet.dabs.len() > 8,
            "whole path recorded, not a press-only dot"
        );
    }
}
fn control(count: usize) -> (App, Vec<u8>) {
    let (mut app, window) = fixture();
    let before = pixels(&app);
    for _ in 0..count {
        for event in gesture(window) {
            batch(&mut app, vec![event]);
        }
    }
    completed(&app, count);
    let after = pixels(&app);
    assert_ne!(before, after, "control must actually paint");
    (app, after)
}

#[test]
fn separate_frame_positive_control_paints_completes_and_undoes() {
    let (mut app, _) = control(1);
    let pipeline = app
        .world_mut()
        .resource_mut::<PaintingResource>()
        .into_inner()
        .get_pipeline_mut(7)
        .unwrap();
    assert!(pipeline.undo());
    // The production extraction system composites restored layer tiles.
    app.update();
    assert!(pixels(&app).iter().all(|b| *b == 0));
}
#[test]
fn whole_gesture_one_batch_matches_separate_frames() {
    let (_, expected) = control(1);
    let (mut app, window) = fixture();
    batch(&mut app, gesture(window));
    completed(&app, 1);
    assert_eq!(pixels(&app), expected);
}
#[test]
fn repeated_complete_batches_release_each_stroke() {
    let (_, expected) = control(2);
    let (mut app, window) = fixture();
    for n in 1..=2 {
        batch(&mut app, gesture(window));
        completed(&app, n);
    }
    assert_eq!(pixels(&app), expected);
}
#[test]
fn two_strokes_one_batch_have_distinct_packets_and_undo_entries() {
    let (_, expected) = control(2);
    let (mut app, window) = fixture();
    let events = gesture(window).into_iter().chain(gesture(window)).collect();
    batch(&mut app, events);
    completed(&app, 2);
    assert_eq!(pixels(&app), expected);
}
#[test]
fn ui_owned_batch_is_consumed_without_later_replay() {
    let (mut app, window) = fixture();
    let before = pixels(&app);
    app.world_mut()
        .resource_mut::<FrontendInputBlockState>()
        .block_pointer = true;
    batch(&mut app, gesture(window));
    app.world_mut()
        .resource_mut::<FrontendInputBlockState>()
        .block_pointer = false;
    for _ in 0..3 {
        batch(&mut app, vec![]);
    }
    let pipeline = app
        .world()
        .resource::<PaintingResource>()
        .get_pipeline(7)
        .unwrap();
    assert_eq!(pipeline.log().total_packet_count(), 0);
    assert_eq!(pipeline.undo_count(), 0);
    assert!(!pipeline.is_stroking());
    assert_eq!(pixels(&app), before);
    assert!(app.world().resource::<PaintMode>().current_stroke.is_none());
}

#[test]
fn complete_gesture_before_focus_loss_is_committed() {
    let (_, expected) = control(1);
    let (mut app, window) = fixture();
    let mut events = gesture(window);
    events.push(WindowEvent::WindowFocused(bevy::window::WindowFocused {
        window,
        focused: false,
    }));
    batch(&mut app, events);
    completed(&app, 1);
    assert_eq!(pixels(&app), expected);
}

#[test]
fn inactive_batch_is_consumed_without_replay_on_entry() {
    let (mut app, window) = fixture();
    let before = pixels(&app);
    app.world_mut().resource_mut::<PaintMode>().active = false;
    batch(&mut app, gesture(window));
    app.world_mut().resource_mut::<PaintMode>().active = true;
    for _ in 0..3 {
        batch(&mut app, vec![]);
    }
    let pipeline = app
        .world()
        .resource::<PaintingResource>()
        .get_pipeline(7)
        .unwrap();
    assert_eq!(pipeline.log().total_packet_count(), 0);
    assert_eq!(pipeline.undo_count(), 0);
    assert_eq!(pixels(&app), before);
    assert!(!pipeline.is_stroking());
    assert!(app.world().resource::<PaintMode>().current_stroke.is_none());
}

#[test]
fn focus_regain_hover_supplies_next_press_origin_without_replaying_input() {
    let (_, expected) = control(1);
    let (mut app, window) = fixture();
    batch(
        &mut app,
        vec![
            WindowEvent::WindowFocused(bevy::window::WindowFocused {
                window,
                focused: false,
            }),
            WindowEvent::WindowFocused(bevy::window::WindowFocused {
                window,
                focused: true,
            }),
            movement(window, 350.),
        ],
    );
    assert!(app.world().resource::<PaintMode>().current_stroke.is_none());
    batch(&mut app, gesture(window).into_iter().skip(1).collect());
    completed(&app, 1);
    assert_eq!(pixels(&app), expected);
}
