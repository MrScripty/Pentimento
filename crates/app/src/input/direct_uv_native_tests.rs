//! Real shared commands, native window batches and the production mesh/canvas plugins.
use bevy::asset::RenderAssetUsages;
use bevy::camera::{ComputedCameraValues, RenderTargetInfo, visibility::VisibilityPlugin};
use bevy::input::touch::{ForceTouch, TouchInput, TouchPhase};
use bevy::input::{
    ButtonState, InputPlugin,
    keyboard::{Key, KeyboardInput},
    mouse::MouseButtonInput,
};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent};
use pentimento_frontend_core::{CaptureResult, CompositeBackend, FrontendError};
use pentimento_ipc::{BevyToUi, PaintCommand, PaintTarget, ProjectCommand, UiToBevy};
use pentimento_ipc::{KeyboardEvent, MouseEvent, UvLayerCommand};
use pentimento_scene::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{cell::RefCell, rc::Rc};
static OWNED: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    app: App,
    window: Entity,
    a: Entity,
    b: Entity,
    canvas: Entity,
    path: PathBuf,
    recorded: Rc<RefCell<Vec<MouseEvent>>>,
}
impl Fixture {
    fn new() -> Self {
        Self::new_with_projection(false)
    }
    fn new_with_projection(projection: bool) -> Self {
        Self::new_projection_sized(projection, 64)
    }
    fn new_projection_sized(projection: bool, resolution: u32) -> Self {
        let root = std::env::var_os("PENTIMENTO_DIRECTUV_DRIVER_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "pentimento-directuv-native-{}-{}",
            std::process::id(),
            OWNED.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            TransformPlugin,
            VisibilityPlugin,
        ))
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(20),
        ))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<OutboundUiMessages>()
        .init_resource::<FrontendInputBlockState>()
        .init_resource::<ActiveCanvasPlane>()
        .init_resource::<EditModeState>()
        .add_message::<WindowEvent>()
        .add_message::<CursorMoved>()
        .add_plugins((
            PaintModePlugin,
            PaintingSystemPlugin,
            MeshPaintModePlugin,
            MeshPaintingSystemPlugin,
        ));
        if projection {
            app.add_message::<AssetEvent<Mesh>>();
            app.add_plugins((
                ProjectionModePlugin,
                ProjectionPaintingPlugin,
                CanvasPlanePlugin,
            ));
        }
        let recorded = Rc::new(RefCell::new(Vec::new()));
        app.init_resource::<pentimento_config::DisplayConfig>()
            .insert_resource(crate::config::PentimentoConfig {
                composite_mode: crate::config::CompositeMode::Cef,
            })
            .insert_resource(FrontendUiLayout {
                received: true,
                ..default()
            })
            .init_resource::<QueuedUi>()
            .add_plugins(super::InputPlugin)
            .add_systems(
                PreUpdate,
                apply_queued_ui.after(super::mouse::forward_native_input),
            );
        app.insert_non_send_resource(crate::render::FrontendResource {
            backend: Box::new(RecordingBackend(recorded.clone())),
            texture_format: TextureFormat::Rgba8Unorm,
        });
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut().spawn((
            MainCamera,
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
            Transform::from_xyz(0., 0., 4.).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        let a = mesh_sized(&mut app, 12, 0., true, resolution);
        let b = mesh(&mut app, 44, 3., false);
        if projection {
            app.world_mut()
                .get_mut::<PaintableMesh>(a)
                .unwrap()
                .storage_mode = painting::MeshStorageMode::UvAtlas {
                resolution: (resolution, resolution),
            };
            app.world_mut()
                .entity_mut(a)
                .insert(ProjectionTarget::uv_atlas((resolution, resolution)));
            app.world_mut()
                .entity_mut(b)
                .insert(ProjectionTarget::uv_atlas((64, 64)));
        }
        let geometry = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::new(2., 2.));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let canvas = app
            .world_mut()
            .spawn((
                Name::new("Source"),
                Mesh3d(geometry),
                MeshMaterial3d(material),
                Transform::from_xyz(0., 0., 0.2),
                CanvasPlane::new(7, resolution, resolution, 2., 2.),
                Visibility::Visible,
            ))
            .id();
        app.world_mut().resource_mut::<ActiveCanvasPlane>().entity = Some(canvas);
        app.world_mut()
            .resource_mut::<ActiveCanvasPlane>()
            .camera_locked = true;
        app.world_mut().resource_mut::<PaintMode>().active = true;
        app.update();
        Self {
            app,
            window,
            a,
            b,
            canvas,
            path,
            recorded,
        }
    }
    fn uv(&mut self, command: UvLayerCommand) {
        self.command(PaintCommand::UvLayers { command });
        self.settle();
    }
    fn layers(&self) -> painting::uv_layers::UvLayersDocument {
        self.app
            .world()
            .resource::<MeshPaintingResource>()
            .uv_layers(12)
            .unwrap()
            .document()
            .clone()
    }
    fn enable_layers(&mut self) {
        self.uv(UvLayerCommand::SelectReceiver { mesh_id: 12 });
        self.uv(UvLayerCommand::Enable);
        assert!(
            self.app
                .world()
                .resource::<MeshPaintingResource>()
                .uv_layers(12)
                .is_some()
        );
    }
    fn command(&mut self, c: PaintCommand) {
        crate::render::dispatch_ui_commands(self.app.world_mut(), [UiToBevy::PaintCommand(c)]);
    }
    fn direct(&mut self) {
        self.command(PaintCommand::SetTarget {
            target: PaintTarget::DirectUv,
        });
        assert_eq!(
            self.app.world().resource::<PaintMode>().target,
            PaintTarget::DirectUv
        );
    }
    fn settle(&mut self) {
        for _ in 0..3 {
            self.app.update();
        }
    }
    fn batch(&mut self, events: Vec<WindowEvent>) {
        for event in events {
            match &event {
                WindowEvent::CursorMoved(e) => {
                    self.app
                        .world_mut()
                        .get_mut::<Window>(e.window)
                        .unwrap()
                        .set_cursor_position(Some(e.position));
                    self.app.world_mut().write_message(e.clone());
                }
                WindowEvent::MouseButtonInput(e) => {
                    self.app.world_mut().write_message(*e);
                }
                WindowEvent::TouchInput(e) => {
                    self.app.world_mut().write_message(*e);
                }
                WindowEvent::KeyboardFocusLost(e) => {
                    self.app.world_mut().write_message(e.clone());
                }
                WindowEvent::KeyboardInput(e) => {
                    self.app.world_mut().write_message(e.clone());
                }
                WindowEvent::WindowFocused(e) => {
                    self.app
                        .world_mut()
                        .get_mut::<Window>(e.window)
                        .unwrap()
                        .focused = e.focused;
                }
                _ => {}
            }
            self.app.world_mut().write_message(event);
        }
        self.app.update();
    }
    fn gesture(&mut self) {
        self.batch(vec![
            movement(self.window, 350.),
            button(self.window, true),
            movement(self.window, 400.),
            movement(self.window, 450.),
            movement(self.window, 500.),
            movement(self.window, 550.),
            movement(self.window, 600.),
            movement(self.window, 650.),
            button(self.window, false),
        ]);
    }
    fn raw(&self) -> Vec<[f32; 4]> {
        self.app
            .world()
            .resource::<MeshPaintingResource>()
            .get_uv_surface(12)
            .unwrap()
            .atlas
            .surface()
            .pixels()
            .to_vec()
    }
    fn image(&self) -> Vec<u8> {
        bound_image(self.app.world(), self.a)
    }
    fn source(&self) -> Vec<u8> {
        self.app
            .world()
            .resource::<PaintingResource>()
            .get_pipeline(7)
            .unwrap()
            .surface_as_bytes()
            .to_vec()
    }
    fn file(&mut self, c: ProjectCommand) -> bool {
        crate::render::dispatch_ui_commands(self.app.world_mut(), [UiToBevy::ProjectCommand(c)]);
        self.app
            .world()
            .resource::<OutboundUiMessages>()
            .messages
            .iter()
            .rev()
            .find_map(|m| {
                if let BevyToUi::ProjectOperationFinished { success, .. } = m {
                    Some(*success)
                } else {
                    None
                }
            })
            .unwrap()
    }
    fn save(&mut self, path: &Path) -> bool {
        self.file(ProjectCommand::Save {
            path: path.to_string_lossy().into_owned(),
        })
    }
    fn open(&mut self, path: &Path) -> bool {
        let okay = self.file(ProjectCommand::Open {
            path: path.to_string_lossy().into_owned(),
        });
        if okay {
            self.a = named(self.app.world_mut(), "UV12");
            self.b = named(self.app.world_mut(), "UV44");
            self.canvas = named(self.app.world_mut(), "Source");
        }
        okay
    }
    fn state(&mut self) -> serde_json::Value {
        crate::render::dispatch_ui_commands(self.app.world_mut(), [UiToBevy::RequestBrushState]);
        let messages = self
            .app
            .world_mut()
            .resource_mut::<OutboundUiMessages>()
            .drain();
        let r = self.app.world().resource::<MeshPaintingResource>();
        let pixels: Vec<_> = r
            .get_uv_surface(12)
            .unwrap()
            .atlas
            .surface()
            .pixels()
            .iter()
            .flatten()
            .map(|v| v.to_bits())
            .collect();
        serde_json::json!({"uv_layers_document":r.uv_layers(12).map(|l|l.document()),"source":self.source(),"native_input_forwarding":true,"messages":messages,"image":self.image(),"pixel_bits":pixels,"project_path":self.path.join("painted.pentimento.json"),"undo":r.undo_count(12),"redo":r.redo_count(12)})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
fn mesh(app: &mut App, id: u32, x: f32, textured: bool) -> Entity {
    mesh_sized(app, id, x, textured, 64)
}
fn mesh_sized(app: &mut App, id: u32, x: f32, textured: bool, resolution: u32) -> Entity {
    let geometry = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Rectangle::new(2., 2.));
    let texture = textured.then(|| {
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new(
                Extent3d {
                    width: resolution,
                    height: resolution,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                (0..resolution * resolution)
                    .flat_map(|i| [(i % resolution * 3) as u8, 128, 200, 175])
                    .collect(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            ))
    });
    let material = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgba(0.2, 0.4, 0.7, 0.8),
            base_color_texture: texture,
            ..default()
        });
    app.world_mut()
        .spawn((
            Name::new(format!("UV{id}")),
            Mesh3d(geometry),
            MeshMaterial3d(material),
            Transform::from_xyz(x, 0., 0.),
            Visibility::Visible,
            PaintableMesh {
                mesh_id: id,
                storage_mode: painting::MeshStorageMode::UvAtlas {
                    resolution: (resolution, resolution),
                },
            },
        ))
        .id()
}
fn bound_image(world: &World, e: Entity) -> Vec<u8> {
    let material = world.get::<MeshMaterial3d<StandardMaterial>>(e).unwrap();
    let material = world
        .resource::<Assets<StandardMaterial>>()
        .get(&material.0)
        .unwrap();
    let image = material.base_color_texture.as_ref().unwrap();
    world
        .resource::<Assets<Image>>()
        .get(image)
        .unwrap()
        .data
        .clone()
        .unwrap()
}
fn appearance(world: &World, e: Entity) -> (Color, Option<Handle<Image>>) {
    let h = &world.get::<MeshMaterial3d<StandardMaterial>>(e).unwrap().0;
    let m = world.resource::<Assets<StandardMaterial>>().get(h).unwrap();
    (m.base_color, m.base_color_texture.clone())
}
fn named(world: &mut World, name: &str) -> Entity {
    world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, n)| n.as_str() == name)
        .unwrap()
        .0
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
fn key(window: Entity, key_code: KeyCode, down: bool) -> WindowEvent {
    WindowEvent::KeyboardInput(KeyboardInput {
        window,
        key_code,
        logical_key: if key_code == KeyCode::Escape {
            Key::Escape
        } else {
            Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified)
        },
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        text: None,
        repeat: false,
    })
}
fn same(a: &[[f32; 4]], b: &[[f32; 4]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .all(|(a, b)| a.to_bits() == b.to_bits())
}

struct RecordingBackend(Rc<RefCell<Vec<MouseEvent>>>);
impl CompositeBackend for RecordingBackend {
    fn poll(&mut self) {}
    fn is_ready(&self) -> bool {
        true
    }
    fn capture_if_dirty(&mut self) -> Option<CaptureResult> {
        None
    }
    fn size(&self) -> (u32, u32) {
        (1000, 1000)
    }
    fn resize(&mut self, _: u32, _: u32) {}
    fn send_mouse_event(&mut self, event: MouseEvent) {
        self.0.borrow_mut().push(event);
    }
    fn send_keyboard_event(&mut self, _: KeyboardEvent) {}
    fn send_to_ui(&mut self, _: BevyToUi) -> Result<(), FrontendError> {
        Ok(())
    }
    fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        None
    }
}
#[derive(Resource, Default)]
struct QueuedUi(Vec<UiToBevy>);
fn apply_queued_ui(world: &mut World) {
    let commands = std::mem::take(&mut world.resource_mut::<QueuedUi>().0);
    crate::render::dispatch_ui_commands(world, commands);
}
fn contact(
    window: Entity,
    id: u64,
    x: f32,
    phase: TouchPhase,
    force: Option<ForceTouch>,
) -> WindowEvent {
    WindowEvent::TouchInput(TouchInput {
        window,
        id,
        position: Vec2::new(x, 500.),
        phase,
        force,
    })
}
fn touch(window: Entity, id: u64, x: f32, phase: TouchPhase, pressure: f64) -> WindowEvent {
    contact(window, id, x, phase, Some(ForceTouch::Normalized(pressure)))
}
fn counts(f: &Fixture) -> (usize, usize) {
    let r = f.app.world().resource::<MeshPaintingResource>();
    (r.undo_count(12), r.redo_count(12))
}
fn pen_stroke(f: &mut Fixture, id: u64, pressure: f64) {
    let w = f.window;
    f.batch(vec![
        touch(w, id, 350., TouchPhase::Started, pressure),
        touch(w, id, 450., TouchPhase::Moved, pressure),
        touch(w, id, 550., TouchPhase::Moved, pressure),
        touch(w, id, 650., TouchPhase::Ended, pressure),
    ]);
    f.settle();
}

#[test]
fn actual_app_mouse_and_pressure_contacts_layer_history_without_touching_canvas() {
    for mode in [
        crate::config::CompositeMode::Capture,
        crate::config::CompositeMode::Cef,
    ] {
        let mut f = Fixture::new();
        f.app
            .world_mut()
            .resource_mut::<crate::config::PentimentoConfig>()
            .composite_mode = mode;
        f.direct();
        f.command(PaintCommand::SetBrushSize { size: 8. });
        f.command(PaintCommand::SetBrushOpacity { opacity: 0.4 });
        let before = f.raw();
        let source = f.source();
        f.gesture();
        f.settle();
        let first = f.raw();
        let first_image = f.image();
        assert!(!same(&before, &first));
        assert_eq!(counts(&f), (1, 0));
        f.command(PaintCommand::SetBrushColor {
            color: [0.8, 0.1, 0.3, 0.7],
        });
        pen_stroke(&mut f, 10, 0.6);
        let second = f.raw();
        let second_image = f.image();
        assert!(!same(&first, &second));
        assert_eq!(counts(&f), (2, 0));
        assert_eq!(f.source(), source);
        f.command(PaintCommand::Undo);
        f.settle();
        assert!(same(&f.raw(), &first));
        assert_eq!(f.image(), first_image);
        f.command(PaintCommand::Redo);
        f.settle();
        assert!(same(&f.raw(), &second));
        assert_eq!(f.image(), second_image);
        assert_eq!(f.source(), source);
    }
}

#[test]
fn contact_first_dab_and_move_use_reported_pressure_and_left_button_is_not_required() {
    let mut low = Fixture::new();
    low.direct();
    low.command(PaintCommand::SelectBrushPreset { preset_id: 2 });
    let mut full = Fixture::new();
    full.direct();
    full.command(PaintCommand::SelectBrushPreset { preset_id: 2 });
    let before = low.raw();
    let w = low.window;
    low.batch(vec![touch(w, 1, 500., TouchPhase::Started, 0.25)]);
    low.settle();
    let w = full.window;
    full.batch(vec![contact(w, 1, 500., TouchPhase::Started, None)]);
    full.settle();
    assert!(
        !low.app
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
    assert!(
        low.app
            .world()
            .resource::<MeshPaintState>()
            .current_stroke
            .is_some()
    );
    assert_eq!(counts(&low), (0, 0));
    let changed =
        |pixels: Vec<[f32; 4]>| pixels.iter().zip(&before).filter(|(a, b)| a != b).count();
    assert!(changed(low.raw()) > 0);
    assert!(changed(low.raw()) < changed(full.raw()));
    let w = low.window;
    low.batch(vec![touch(w, 1, 580., TouchPhase::Moved, 0.9)]);
    assert!(
        low.app
            .world()
            .resource::<MeshPaintState>()
            .current_stroke
            .is_some()
    );
    low.batch(vec![contact(w, 1, 580., TouchPhase::Ended, None)]);
    low.settle();
    assert_eq!(counts(&low), (1, 0));
    assert_eq!(
        touch_pressure(Some(ForceTouch::Calibrated {
            force: 2.,
            max_possible_force: 4.,
            altitude_angle: Some(0.5)
        })),
        Some(0.5)
    );
    for force in [
        ForceTouch::Normalized(f64::NAN),
        ForceTouch::Normalized(-1.),
        ForceTouch::Normalized(2.),
        ForceTouch::Calibrated {
            force: 1.,
            max_possible_force: 0.,
            altitude_angle: None,
        },
    ] {
        assert_eq!(touch_pressure(Some(force)), None);
    }
}

#[test]
fn native_cancel_escape_and_invalid_samples_restore_exact_pixels_and_keep_redo() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    pen_stroke(&mut f, 1, 0.5);
    f.command(PaintCommand::Undo);
    f.settle();
    let raw = f.raw();
    let image = f.image();
    let binding = appearance(f.app.world(), f.a);
    for cancellation in [0, 1, 2, 3] {
        let w = f.window;
        f.batch(vec![
            touch(w, 2, 450., TouchPhase::Started, 0.7),
            touch(w, 2, 550., TouchPhase::Moved, 0.8),
        ]);
        assert!(
            f.app
                .world()
                .resource::<MeshPaintState>()
                .current_stroke
                .is_some()
        );
        match cancellation {
            0 => f.batch(vec![touch(w, 2, 550., TouchPhase::Canceled, 0.8)]),
            1 => {
                f.batch(vec![
                    key(w, KeyCode::Escape, true),
                    key(w, KeyCode::Escape, false),
                ]);
                f.batch(vec![
                    touch(w, 2, 580., TouchPhase::Moved, 0.8),
                    touch(w, 2, 580., TouchPhase::Ended, 0.8),
                ]);
            }
            2 => f.batch(vec![
                touch(w, 2, 550., TouchPhase::Moved, f64::NAN),
                touch(w, 2, 550., TouchPhase::Ended, 0.8),
            ]),
            _ => {
                f.command(PaintCommand::CancelStroke);
                f.settle();
                f.batch(vec![
                    touch(w, 2, 580., TouchPhase::Moved, 0.8),
                    touch(w, 2, 580., TouchPhase::Ended, 0.8),
                ]);
            }
        }
        f.settle();
        assert!(same(&raw, &f.raw()));
        assert_eq!(image, f.image());
        assert_eq!(binding, appearance(f.app.world(), f.a));
        assert_eq!(counts(&f), (0, 1));
    }
    pen_stroke(&mut f, 3, 0.5);
    assert_eq!(counts(&f), (1, 0));
}

#[test]
fn contact_capture_ui_origin_second_contact_and_companion_mouse_cannot_steal_owner() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    f.app.world_mut().resource_mut::<FrontendUiLayout>().regions =
        vec![pentimento_ipc::LayoutRegion {
            id: "panel".into(),
            x: 600.,
            y: 0.,
            width: 400.,
            height: 1000.,
            z_index: 1,
            accepts_keyboard: true,
        }];
    let raw = f.raw();
    let w = f.window;
    f.batch(vec![
        touch(w, 1, 700., TouchPhase::Started, 0.5),
        touch(w, 1, 450., TouchPhase::Moved, 0.5),
        touch(w, 1, 450., TouchPhase::Ended, 0.5),
    ]);
    f.settle();
    assert!(same(&raw, &f.raw()));
    assert_eq!(counts(&f), (0, 0));
    let packets = f.recorded.borrow();
    assert!(
        packets
            .iter()
            .any(|e| matches!(e, MouseEvent::ButtonDown { .. }))
    );
    assert!(
        packets
            .iter()
            .any(|e| matches!(e, MouseEvent::ButtonUp { .. }))
    );
    drop(packets);
    f.batch(vec![touch(w, 2, 450., TouchPhase::Started, 0.4)]);
    f.batch(vec![
        touch(w, 3, 500., TouchPhase::Started, 1.),
        touch(w, 3, 500., TouchPhase::Ended, 1.),
        button(w, true),
        movement(w, 580.),
        button(w, false),
    ]);
    assert!(
        f.app
            .world()
            .resource::<MeshPaintState>()
            .current_stroke
            .is_some()
    );
    assert_eq!(counts(&f), (0, 0));
    f.batch(vec![
        touch(w, 2, 550., TouchPhase::Moved, 0.6),
        touch(w, 2, 700., TouchPhase::Moved, 0.6),
    ]);
    f.settle();
    let prefix = f.raw();
    assert!(!same(&prefix, &raw));
    assert_eq!(counts(&f), (1, 0));
    f.batch(vec![
        touch(w, 2, 450., TouchPhase::Moved, 0.8),
        touch(w, 2, 450., TouchPhase::Ended, 0.8),
    ]);
    f.settle();
    assert!(same(&prefix, &f.raw()));
    assert_eq!(counts(&f), (1, 0));
}

#[test]
fn focus_loss_cancels_contact_and_keyboard_before_loss_keeps_prior_modifier_chronology() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    let before = f.raw();
    let w = f.window;
    f.batch(vec![
        touch(w, 1, 450., TouchPhase::Started, 0.7),
        touch(w, 1, 550., TouchPhase::Moved, 0.7),
    ]);
    f.batch(vec![WindowEvent::WindowFocused(
        bevy::window::WindowFocused {
            window: w,
            focused: false,
        },
    )]);
    f.settle();
    assert!(same(&before, &f.raw()));
    assert_eq!(counts(&f), (0, 0));
    f.batch(vec![
        WindowEvent::WindowFocused(bevy::window::WindowFocused {
            window: w,
            focused: true,
        }),
        touch(w, 1, 580., TouchPhase::Moved, 0.7),
        touch(w, 1, 580., TouchPhase::Ended, 0.7),
    ]);
    assert_eq!(counts(&f), (0, 0));
    pen_stroke(&mut f, 2, 0.7);
    f.batch(vec![key(w, KeyCode::ControlLeft, true)]);
    f.batch(vec![
        key(w, KeyCode::KeyZ, true),
        key(w, KeyCode::KeyZ, false),
        WindowEvent::KeyboardFocusLost(bevy::input::keyboard::KeyboardFocusLost),
    ]);
    f.settle();
    assert_eq!(counts(&f), (0, 1));
    assert!(same(&before, &f.raw()));
}

#[test]
fn stationary_press_after_ui_hides_uses_current_origin_and_queued_target_cannot_reinterpret_prefix()
{
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    let w = f.window;
    f.batch(vec![movement(w, 450.)]);
    f.app.world_mut().resource_mut::<FrontendUiLayout>().regions =
        vec![pentimento_ipc::LayoutRegion {
            id: "modal".into(),
            x: 0.,
            y: 0.,
            width: 1000.,
            height: 1000.,
            z_index: 10,
            accepts_keyboard: true,
        }];
    f.batch(vec![movement(w, 550.)]);
    f.app
        .world_mut()
        .resource_mut::<FrontendUiLayout>()
        .regions
        .clear();
    f.app
        .world_mut()
        .resource_mut::<QueuedUi>()
        .0
        .push(UiToBevy::PaintCommand(PaintCommand::SetTarget {
            target: PaintTarget::Canvas,
        }));
    f.batch(vec![button(w, true), button(w, false)]);
    f.settle();
    let stationary = f.raw();
    assert_eq!(
        f.app.world().resource::<PaintMode>().target,
        PaintTarget::DirectUv
    );
    assert_eq!(counts(&f), (1, 0));
    let mut control = Fixture::new();
    control.direct();
    control.command(PaintCommand::SetBrushSize { size: 8. });
    let w = control.window;
    control.batch(vec![movement(w, 550.), button(w, true), button(w, false)]);
    control.settle();
    assert!(same(&stationary, &control.raw()));
}

#[test]
fn contact_end_then_native_shortcut_one_batch_and_geometry_conflict_are_safe() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    let before = f.raw();
    let w = f.window;
    f.batch(vec![
        touch(w, 1, 450., TouchPhase::Started, 0.5),
        touch(w, 1, 550., TouchPhase::Ended, 0.8),
        key(w, KeyCode::ControlLeft, true),
        key(w, KeyCode::KeyZ, true),
        key(w, KeyCode::KeyZ, false),
        key(w, KeyCode::ControlLeft, false),
    ]);
    f.settle();
    assert_eq!(counts(&f), (0, 1));
    assert!(same(&before, &f.raw()));
    let h = f.app.world().get::<Mesh3d>(f.a).unwrap().0.clone();
    f.batch(vec![touch(w, 2, 450., TouchPhase::Started, 0.6)]);
    let admitted_prefix = f.raw();
    let mut meshes = f.app.world_mut().resource_mut::<Assets<Mesh>>();
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) = meshes
        .get_mut(&h)
        .unwrap()
        .attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        points[0][2] = 0.125;
    }
    drop(meshes);
    f.batch(vec![
        touch(w, 2, 550., TouchPhase::Moved, 0.6),
        touch(w, 2, 550., TouchPhase::Ended, 0.6),
    ]);
    f.settle();
    // Existing ownership conflict admission retains current bytes, abandons
    // pending history, and refuses subsequent dabs. It never rewinds geometry.
    assert!(same(&admitted_prefix, &f.raw()));
    assert_eq!(counts(&f), (0, 0));
    assert!(!undo_mesh_paint(f.app.world_mut(), f.a));
    let points = f
        .app
        .world()
        .resource::<Assets<Mesh>>()
        .get(&h)
        .unwrap()
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap();
    assert!(matches!(points,bevy::mesh::VertexAttributeValues::Float32x3(p)if p[0][2]==0.125));
}

#[test]
#[ignore = "real Svelte -> native app input forwarding -> production CPU assets driver"]
fn browser_driver() {
    use std::io::{BufRead, Write};
    let mut f =
        Fixture::new_with_projection(std::env::var_os("PENTIMENTO_UV_LAYERS_DRIVER").is_some());
    println!("PENTIMENTO_DRIVER {}", f.state());
    std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        let input: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        if input["stop"].as_bool() == Some(true) {
            break;
        }
        if let Some(ui) = input.get("ui") {
            let command: UiToBevy = serde_json::from_value(ui.clone()).unwrap();
            crate::render::dispatch_ui_commands(f.app.world_mut(), [command]);
            f.app.update();
            if f.app.world().get_entity(f.a).is_err() {
                f.a = named(f.app.world_mut(), "UV12");
                f.b = named(f.app.world_mut(), "UV44");
                f.canvas = named(f.app.world_mut(), "Source");
            }
        }
        if let Some(pointer) = input.get("pointer") {
            let x = pointer["x"].as_f64().unwrap_or(500.) as f32;
            let kind = pointer["kind"].as_str().unwrap();
            let event = if pointer["pointerType"].as_str() == Some("pen") {
                touch(
                    f.window,
                    pointer["id"].as_u64().unwrap_or(8),
                    x,
                    match kind {
                        "down" => TouchPhase::Started,
                        "move" => TouchPhase::Moved,
                        "up" => TouchPhase::Ended,
                        "cancel" => TouchPhase::Canceled,
                        _ => panic!("unknown pen phase"),
                    },
                    pointer["pressure"].as_f64().unwrap_or(1.),
                )
            } else {
                match kind {
                    "move" => movement(f.window, x),
                    "down" => button(f.window, true),
                    "up" => button(f.window, false),
                    _ => panic!("unknown mouse phase"),
                }
            };
            f.batch(vec![event]);
        }
        if let Some(action) = input.get("key") {
            let code = match action["code"].as_str().unwrap() {
                "Escape" => KeyCode::Escape,
                "ShiftLeft" => KeyCode::ShiftLeft,
                "Tab" => KeyCode::Tab,
                "ControlLeft" => KeyCode::ControlLeft,
                "KeyZ" => KeyCode::KeyZ,
                _ => panic!("unknown key"),
            };
            f.batch(vec![key(f.window, code, action["down"].as_bool().unwrap())]);
        }
        f.settle();
        println!("PENTIMENTO_DRIVER {}", f.state());
        std::io::stdout().flush().unwrap();
    }
}

#[test]
fn actual_native_contact_save_open_reentry_preserves_file_and_rejects_old_held_input() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    pen_stroke(&mut f, 1, 0.5);
    f.command(PaintCommand::SetBrushColor {
        color: [0.1, 0.4, 0.9, 0.6],
    });
    pen_stroke(&mut f, 2, 0.8);
    let raw = f.raw();
    let image = f.image();
    let source = f.source();
    let w = f.window;
    let file = f.path.join("native-owned.pentimento.json");
    f.batch(vec![touch(w, 22, 450., TouchPhase::Started, 0.5)]);
    assert!(!f.save(&file));
    assert!(!file.exists());
    f.command(PaintCommand::CancelStroke);
    f.settle();
    assert!(same(&raw, &f.raw()));
    assert!(f.save(&file));
    assert!(f.open(&file));
    f.settle();
    assert!(same(&raw, &f.raw()));
    assert_eq!(image, f.image());
    assert_eq!(source, f.source());
    assert_eq!(counts(&f), (0, 0));
    f.batch(vec![
        key(w, KeyCode::ShiftLeft, true),
        key(w, KeyCode::Tab, true),
    ]);
    f.batch(vec![
        key(w, KeyCode::Tab, false),
        key(w, KeyCode::ShiftLeft, false),
    ]);
    f.direct();
    f.batch(vec![
        touch(w, 22, 550., TouchPhase::Moved, 0.8),
        touch(w, 22, 550., TouchPhase::Ended, 0.8),
    ]);
    f.settle();
    assert!(same(&raw, &f.raw()));
    assert_eq!(counts(&f), (0, 0));
    pen_stroke(&mut f, 23, 0.5);
    assert_eq!(counts(&f), (1, 0));
    f.command(PaintCommand::Undo);
    f.settle();
    assert!(same(&raw, &f.raw()));
    assert_eq!(image, f.image());
    let external = b"external owner";
    std::fs::write(&file, external).unwrap();
    assert!(!f.save(&file));
    assert_eq!(std::fs::read(&file).unwrap(), external);
}

#[test]
fn native_contact_dpi_ui_capture_and_canvas_ptex_mode_isolation_are_explicit() {
    for mode in [
        crate::config::CompositeMode::Capture,
        crate::config::CompositeMode::Cef,
    ] {
        let mut f = Fixture::new();
        f.direct();
        let before = f.raw();
        let w = f.window;
        f.app
            .world_mut()
            .resource_mut::<crate::config::PentimentoConfig>()
            .composite_mode = mode;
        f.app
            .world_mut()
            .get_mut::<Window>(w)
            .unwrap()
            .resolution
            .set_scale_factor_override(Some(2.));
        f.app.world_mut().resource_mut::<FrontendUiLayout>().regions =
            vec![pentimento_ipc::LayoutRegion {
                id: "panel".into(),
                x: 600.,
                y: 40.,
                width: 400.,
                height: 1000.,
                z_index: 1,
                accepts_keyboard: true,
            }];
        let x = if mode == crate::config::CompositeMode::Cef {
            700.
        } else {
            350.
        };
        f.batch(vec![
            touch(w, 1, x, TouchPhase::Started, 0.5),
            touch(w, 1, x, TouchPhase::Ended, 0.5),
        ]);
        f.settle();
        assert!(same(&before, &f.raw()));
        assert_eq!(counts(&f), (0, 0));
        assert!(
            f.recorded
                .borrow()
                .iter()
                .any(|e| matches!(e,MouseEvent::ButtonDown{x,..}if *x==700.))
        );
    }
    let mut f = Fixture::new();
    let before = f.raw();
    let source = f.source();
    pen_stroke(&mut f, 1, 0.7);
    assert!(same(&before, &f.raw()));
    assert_eq!(source, f.source());
    assert_eq!(counts(&f), (0, 0));
    f.app
        .world_mut()
        .get_mut::<PaintableMesh>(f.a)
        .unwrap()
        .storage_mode = painting::MeshStorageMode::Ptex { face_resolution: 8 };
    f.settle();
    f.direct();
    pen_stroke(&mut f, 2, 0.7);
    assert!(same(&before, &f.raw()));
    assert_eq!(counts(&f), (0, 0));
    assert!(
        f.app
            .world()
            .resource::<PaintMode>()
            .target_notice
            .as_deref()
            .unwrap()
            .contains("PTex")
    );
}

#[test]
fn ui_filtered_modifier_release_does_not_leave_a_sticky_direct_history_shortcut() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    pen_stroke(&mut f, 1, 0.5);
    let raw = f.raw();
    let w = f.window;
    f.batch(vec![key(w, KeyCode::ControlLeft, true)]);
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [UiToBevy::SetUiInputCapture { keyboard: true }],
    );
    f.batch(vec![key(w, KeyCode::ControlLeft, false)]);
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [UiToBevy::SetUiInputCapture { keyboard: false }],
    );
    f.batch(vec![
        key(w, KeyCode::KeyZ, true),
        key(w, KeyCode::KeyZ, false),
    ]);
    f.settle();
    assert_eq!(counts(&f), (1, 0));
    assert!(same(&raw, &f.raw()));
}

#[test]
fn pending_mouse_hover_precedes_ui_touch_and_cannot_replay_while_contact_is_held() {
    let mut f = Fixture::new();
    let w = f.window;
    f.app.world_mut().resource_mut::<FrontendUiLayout>().regions =
        vec![pentimento_ipc::LayoutRegion {
            id: "panel".into(),
            x: 600.,
            y: 40.,
            width: 400.,
            height: 1000.,
            z_index: 1,
            accepts_keyboard: true,
        }];
    f.recorded.borrow_mut().clear();
    f.batch(vec![
        movement(w, 300.),
        touch(w, 1, 700., TouchPhase::Started, 0.5),
    ]);
    f.settle();
    let packets = f.recorded.borrow();
    assert!(matches!(
        packets.as_slice(),
        [
            MouseEvent::Move { x: 300., .. },
            MouseEvent::Move { x: 700., .. },
            MouseEvent::ButtonDown { x: 700., .. }
        ]
    ));
    drop(packets);
    f.batch(vec![
        movement(w, 200.),
        touch(w, 1, 750., TouchPhase::Moved, 0.5),
    ]);
    f.settle();
    assert!(
        !f.recorded
            .borrow()
            .iter()
            .any(|e| matches!(e, MouseEvent::Move { x: 200., .. }))
    );
    assert!(matches!(
        f.recorded.borrow().last(),
        Some(MouseEvent::Move { x: 750., .. })
    ));
    f.batch(vec![touch(w, 1, 750., TouchPhase::Ended, 0.5)]);
}

#[test]
fn focus_reset_releases_retained_direct_history_modifiers() {
    for keyboard_loss in [false, true] {
        let mut f = Fixture::new();
        f.direct();
        f.command(PaintCommand::SetBrushSize { size: 8. });
        pen_stroke(&mut f, 1, 0.5);
        let raw = f.raw();
        let w = f.window;
        f.batch(vec![
            key(w, KeyCode::ControlLeft, true),
            key(w, KeyCode::ShiftLeft, true),
        ]);
        f.batch(vec![if keyboard_loss {
            WindowEvent::KeyboardFocusLost(bevy::input::keyboard::KeyboardFocusLost)
        } else {
            WindowEvent::WindowFocused(bevy::window::WindowFocused {
                window: w,
                focused: false,
            })
        }]);
        f.batch(vec![
            WindowEvent::WindowFocused(bevy::window::WindowFocused {
                window: w,
                focused: true,
            }),
            key(w, KeyCode::KeyZ, true),
            key(w, KeyCode::KeyZ, false),
        ]);
        f.settle();
        assert_eq!(counts(&f), (1, 0));
        assert!(same(&raw, &f.raw()));
        f.batch(vec![
            key(w, KeyCode::ControlLeft, true),
            key(w, KeyCode::KeyZ, true),
            key(w, KeyCode::KeyZ, false),
            key(w, KeyCode::ControlLeft, false),
        ]);
        f.settle();
        assert_eq!(counts(&f), (0, 1));
    }
}

#[test]
fn shared_blends_actual_modes_history_upload_and_owned_reopen() {
    use pentimento_ipc::UvLayerBlendMode as Mode;
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.direct();
    f.command(PaintCommand::SetBrushOpacity { opacity: 0.45 });
    f.command(PaintCommand::SetBrushColor {
        color: [0.8, 0.3, 0.5, 1.],
    });
    f.gesture();
    f.settle();
    f.uv(UvLayerCommand::Create {
        name: "Blended".into(),
    });
    let id = f.layers().active_layer;
    f.command(PaintCommand::SetBrushColor {
        color: [0.2, 0.7, 0.4, 1.],
    });
    f.gesture();
    f.settle();
    let raw = f
        .layers()
        .layers
        .iter()
        .map(|l| l.pixels.clone())
        .collect::<Vec<_>>();
    let mut images = Vec::new();
    for mode in [Mode::Normal, Mode::Multiply, Mode::Screen, Mode::Overlay] {
        let before = f.layers();
        let before_image = f.image();
        let n = counts(&f).0;
        f.uv(UvLayerCommand::BlendMode { layer_id: id, mode });
        let after = f.layers();
        let display = f.image();
        assert_eq!(
            raw,
            after
                .layers
                .iter()
                .map(|l| l.pixels.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(counts(&f).0, n + usize::from(mode != Mode::Normal));
        let state = f.state();
        let ui = state["messages"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|m| m["type"] == "PaintBrushStateChanged")
            .unwrap();
        assert_eq!(
            ui["data"]["target"]["uv_layers"]["layers"][0]["blend_mode"],
            serde_json::to_value(mode).unwrap()
        );
        if mode != Mode::Normal {
            assert_ne!(display, before_image);
            f.uv(UvLayerCommand::Undo);
            assert_eq!(f.layers(), before);
            assert_eq!(f.image(), before_image);
            f.uv(UvLayerCommand::Redo);
            assert_eq!(f.layers(), after);
            assert_eq!(f.image(), display);
        }
        let path = f.path.join(format!("blend-{mode:?}.json"));
        assert!(f.save(&path));
        f.uv(UvLayerCommand::Rename {
            layer_id: id,
            name: "Temporary".into(),
        });
        assert!(f.open(&path));
        f.settle();
        assert_eq!(f.layers(), after);
        assert_eq!(f.image(), display);
        assert_eq!(counts(&f), (0, 0));
        images.push(display);
    }
    for a in 0..images.len() {
        for b in a + 1..images.len() {
            assert_ne!(
                images[a], images[b],
                "modes must reach the actual display uploader"
            );
        }
    }
}

#[test]
fn shared_blends_actual_mode_order_opacity_history_and_reopen_after_repairs() {
    use pentimento_ipc::UvLayerBlendMode as Mode;
    for mode in [Mode::Normal, Mode::Multiply, Mode::Screen, Mode::Overlay] {
        let mut f = Fixture::new_with_projection(true);
        f.enable_layers();
        f.direct();
        f.command(PaintCommand::SetBrushOpacity { opacity: 0.45 });
        f.command(PaintCommand::SetBrushColor {
            color: [0.8, 0.3, 0.5, 1.],
        });
        f.gesture();
        f.settle();
        f.uv(UvLayerCommand::Create {
            name: "Upper".into(),
        });
        let id = f.layers().active_layer;
        f.command(PaintCommand::SetBrushColor {
            color: [0.2, 0.7, 0.4, 1.],
        });
        f.gesture();
        f.settle();
        f.uv(UvLayerCommand::BlendMode { layer_id: id, mode });
        let before = f.layers();
        let before_image = f.image();
        let raw = before
            .layers
            .iter()
            .map(|l| (l.meta.id, l.pixels.clone()))
            .collect::<Vec<_>>();
        // Production gestures have empty texels and partially transparent brush edges.
        assert!(raw.iter().any(|(_, p)| p.iter().any(|v| v[3] == 0.)));
        assert!(
            raw.iter()
                .any(|(_, p)| p.iter().any(|v| v[3] > 0. && v[3] < 1.))
        );
        f.uv(UvLayerCommand::Opacity {
            layer_id: id,
            opacity: 0.35,
        });
        let faded = f.layers();
        let faded_image = f.image();
        assert_ne!(faded_image, before_image);
        assert_eq!(faded.compositor, before.compositor);
        f.uv(UvLayerCommand::Reorder {
            layer_id: id,
            new_index: 0,
        });
        let reordered = f.layers();
        let reordered_image = f.image();
        assert_ne!(reordered_image, faded_image);
        for (layer_id, pixels) in &raw {
            assert!(painting::uv_layers::same_uv_pixels(
                pixels,
                &reordered
                    .layers
                    .iter()
                    .find(|l| l.meta.id == *layer_id)
                    .unwrap()
                    .pixels
            ));
        }
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), faded);
        assert_eq!(f.image(), faded_image);
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), before);
        assert_eq!(f.image(), before_image);
        f.uv(UvLayerCommand::Redo);
        f.uv(UvLayerCommand::Redo);
        assert_eq!(f.layers(), reordered);
        assert_eq!(f.image(), reordered_image);
        let path = f.path.join(format!("combined-{mode:?}.json"));
        assert!(f.save(&path));
        f.uv(UvLayerCommand::BlendMode {
            layer_id: id,
            mode: if mode == Mode::Normal {
                Mode::Multiply
            } else {
                Mode::Normal
            },
        });
        assert!(f.open(&path));
        f.settle();
        assert_eq!(f.layers(), reordered);
        assert_eq!(f.image(), reordered_image);
        assert_eq!(counts(&f), (0, 0));
    }
}

#[test]
fn shared_blends_actual_direct_projection_and_live_use_same_layer_modes() {
    use pentimento_ipc::UvLayerBlendMode as Mode;
    for mode in [Mode::Normal, Mode::Multiply, Mode::Screen, Mode::Overlay] {
        let mut f = Fixture::new_with_projection(true);
        f.enable_layers();
        f.direct();
        f.command(PaintCommand::SetBrushColor {
            color: [0.8, 0.3, 0.5, 1.],
        });
        f.gesture();
        f.settle();
        f.uv(UvLayerCommand::Create {
            name: "Projection".into(),
        });
        let id = f.layers().active_layer;
        f.uv(UvLayerCommand::BlendMode { layer_id: id, mode });
        f.command(PaintCommand::SetBrushColor {
            color: [0.2, 0.7, 0.4, 1.],
        });
        f.gesture();
        f.settle();
        let direct = f.layers();
        let image = f.image();
        f.uv(UvLayerCommand::Undo);
        f.uv(UvLayerCommand::Redo);
        assert_eq!(f.layers(), direct);
        assert_eq!(f.image(), image);
        f.command(PaintCommand::SetTarget {
            target: PaintTarget::Canvas,
        });
        f.gesture();
        f.settle();
        let source = f.source();
        let before = f.layers();
        let n = counts(&f).0;
        let committed = f.image();
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        let preview = f.image();
        assert_ne!(preview, committed);
        assert_eq!(f.layers(), before);
        f.uv(UvLayerCommand::BlendMode {
            layer_id: id,
            mode: Mode::Normal,
        });
        assert_eq!(f.layers(), before);
        f.command(PaintCommand::ProjectToScene);
        f.settle();
        assert_eq!(f.image(), preview);
        assert_eq!(counts(&f).0, n + 1);
        assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
        let applied = f.layers();
        assert_eq!(applied.layers[0], before.layers[0]);
        assert_eq!(applied.layers[1].meta, before.layers[1].meta);
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), before);
        assert_eq!(f.image(), committed);
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        f.command(PaintCommand::CancelUvProjection);
        f.settle();
        assert_eq!(f.layers(), before);
        assert_eq!(counts(&f).1, 1);
        f.uv(UvLayerCommand::Redo);
        assert_eq!(f.layers(), applied);
        assert_eq!(f.image(), preview);
        assert_eq!(f.source(), source);
        f.uv(UvLayerCommand::Lock {
            layer_id: id,
            locked: true,
        });
        let locked = f.layers();
        f.command(PaintCommand::ProjectToScene);
        f.settle();
        assert_eq!(f.layers(), locked);
        f.uv(UvLayerCommand::Lock {
            layer_id: id,
            locked: false,
        });
        f.uv(UvLayerCommand::Visible {
            layer_id: id,
            visible: false,
        });
        let hidden = f.layers();
        f.direct();
        f.gesture();
        f.settle();
        assert_eq!(f.layers(), hidden);
    }
}

#[test]
fn shared_blends_actual_v3_legacy_defaults_and_bad_mode_policy_open_atomically() {
    use pentimento_ipc::UvLayerBlendMode as Mode;
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.direct();
    f.gesture();
    f.settle();
    let path = f.path.join("old-v3.json");
    assert!(f.save(&path));
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let uv = saved["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["uv_layers"].is_object())
        .unwrap()
        .get_mut("uv_layers")
        .unwrap();
    for layer in uv["layers"].as_array_mut().unwrap() {
        layer["meta"].as_object_mut().unwrap().remove("blend_mode");
    }
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    let legacy = f.layers();
    let original_image = f.image();
    assert!(f.open(&path));
    f.settle();
    assert_eq!(f.layers(), legacy);
    assert_eq!(f.image(), original_image);
    f.uv(UvLayerCommand::BlendMode {
        layer_id: legacy.active_layer,
        mode: Mode::Overlay,
    });
    let preserved = f.layers();
    let image = f.image();
    let history = counts(&f);
    for bad in ["unknown-mode", "legacy-marker", "unknown-policy"] {
        let mut invalid = saved.clone();
        let uv = invalid["objects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|o| o["uv_layers"].is_object())
            .unwrap()
            .get_mut("uv_layers")
            .unwrap();
        match bad {
            "unknown-mode" => {
                uv["compositor"] = serde_json::json!(painting::uv_layers::UV_BLEND_COMPOSITOR);
                uv["layers"][0]["meta"]["blend_mode"] = serde_json::json!("SoftLight");
            }
            "legacy-marker" => {
                uv["layers"][0]["meta"]["blend_mode"] = serde_json::json!("Multiply")
            }
            _ => uv["compositor"] = serde_json::json!("unknown"),
        }
        let badpath = f.path.join(format!("{bad}.json"));
        std::fs::write(&badpath, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(!f.open(&badpath));
        assert_eq!(f.layers(), preserved);
        assert_eq!(f.image(), image);
        assert_eq!(counts(&f), history);
    }
}

#[test]
fn shared_layers_actual_authoring_selection_structural_history_and_persistence() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let initial = f.layers();
    let base = initial.active_layer;
    f.uv(UvLayerCommand::Create {
        name: "Detail".into(),
    });
    let detail = f.layers().active_layer;
    f.direct();
    f.gesture();
    f.settle();
    let painted = f.layers();
    assert!(
        painted
            .layers
            .iter()
            .find(|l| l.meta.id == detail)
            .unwrap()
            .pixels
            .iter()
            .any(|p| p[3] > 0.)
    );
    assert_eq!(painted.layers[0].pixels, initial.layers[0].pixels);
    let display = f.image();
    f.uv(UvLayerCommand::Undo);
    assert_ne!(f.layers(), painted);
    f.uv(UvLayerCommand::Select { layer_id: base });
    assert_eq!(counts(&f).1, 1, "view selection must retain redo");
    f.uv(UvLayerCommand::Redo);
    assert_eq!(f.layers(), painted);
    assert_eq!(f.image(), display);
    f.uv(UvLayerCommand::Duplicate { layer_id: detail });
    let copy = f.layers().active_layer;
    assert_eq!(
        f.layers().layers.last().unwrap().pixels,
        painted.layers.last().unwrap().pixels
    );
    f.uv(UvLayerCommand::Rename {
        layer_id: copy,
        name: "Soft detail".into(),
    });
    f.uv(UvLayerCommand::Opacity {
        layer_id: copy,
        opacity: 0.25,
    });
    f.uv(UvLayerCommand::Visible {
        layer_id: detail,
        visible: false,
    });
    f.uv(UvLayerCommand::Lock {
        layer_id: copy,
        locked: true,
    });
    let locked = f.layers();
    f.gesture();
    f.settle();
    assert_eq!(
        f.layers(),
        locked,
        "locked layer must refuse an actual gesture"
    );
    f.uv(UvLayerCommand::Reorder {
        layer_id: copy,
        new_index: 0,
    });
    let reordered = f.layers();
    assert_eq!(reordered.layers[0].meta.id, copy);
    f.uv(UvLayerCommand::Delete { layer_id: copy });
    f.uv(UvLayerCommand::Undo);
    assert_eq!(
        f.layers(),
        reordered,
        "delete undo restores pixels, metadata and selection"
    );
    let path = f.path.join("shared.pentimento.json");
    assert!(f.save(&path));
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["version"], 3);
    assert!(saved["uv_receiver"].is_number());
    f.uv(UvLayerCommand::Delete { layer_id: detail });
    assert!(f.open(&path));
    f.settle();
    assert_eq!(f.layers(), reordered);
    assert_eq!(counts(&f), (0, 0), "Open starts fresh history");
    assert_eq!(
        f.app.world().resource::<PaintMode>().direct_target,
        Some(f.a)
    );
    assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
    assert_eq!(
        f.app
            .world()
            .get::<ProjectionTarget>(f.a)
            .unwrap()
            .texture_handle,
        Some(
            f.app
                .world()
                .get::<MeshPaintTexture>(f.a)
                .unwrap()
                .image_handle
                .clone()
        )
    );
}

#[test]
fn shared_layers_actual_canvas_apply_and_direct_share_selected_authoring_and_history() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let base = f.layers().active_layer;
    f.gesture(); // actual native Canvas source painting
    f.settle();
    assert!(f.source().iter().any(|v| *v != 0));
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    let projected = f.layers();
    assert!(projected.layers[0].pixels.iter().any(|p| p[3] > 0.));
    assert_eq!(counts(&f), (1, 0), "one Apply is one UV commit");
    f.uv(UvLayerCommand::Create {
        name: "Direct".into(),
    });
    let direct = f.layers().active_layer;
    f.direct();
    f.gesture();
    f.settle();
    let mixed = f.layers();
    assert_eq!(mixed.layers[0].pixels, projected.layers[0].pixels);
    assert!(mixed.layers[1].pixels.iter().any(|p| p[3] > 0.));
    f.command(PaintCommand::SetTarget {
        target: PaintTarget::Canvas,
    });
    f.settle();
    f.uv(UvLayerCommand::Select { layer_id: base });
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    let twice = f.layers();
    assert_eq!(twice.layers[1].pixels, mixed.layers[1].pixels);
    assert_ne!(twice.layers[0].pixels, mixed.layers[0].pixels);
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers().layers, mixed.layers);
    f.uv(UvLayerCommand::Redo);
    assert_eq!(f.layers(), twice);
    let source = f.source();
    f.command(PaintCommand::Undo);
    f.settle();
    assert_ne!(f.source(), source, "Canvas Undo edits source only");
    assert_eq!(f.layers(), twice);
    f.uv(UvLayerCommand::Select { layer_id: direct });
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    assert!(f.app.world().resource::<ProjectionMode>().live_projection);
    assert_eq!(f.layers().layers, twice.layers);
    f.command(PaintCommand::CancelUvProjection);
    f.settle();
    assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
    assert_eq!(f.layers().layers, twice.layers);
}

#[test]
fn shared_layers_actual_pending_authoring_blocks_selection_and_external_image_conflict() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.uv(UvLayerCommand::Create {
        name: "Frozen stroke".into(),
    });
    let selected = f.layers().active_layer;
    f.direct();
    f.batch(vec![movement(f.window, 500.), button(f.window, true)]);
    assert!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .has_active_stroke()
    );
    f.uv(UvLayerCommand::Select { layer_id: 1 });
    assert_eq!(f.layers().active_layer, selected);
    let handle = f
        .app
        .world()
        .get::<MeshPaintTexture>(f.a)
        .unwrap()
        .image_handle
        .clone();
    let edited = {
        let mut images = f.app.world_mut().resource_mut::<Assets<Image>>();
        let data = images.get_mut(&handle).unwrap().data.as_mut().unwrap();
        data[0] ^= 127;
        data.clone()
    };
    f.batch(vec![movement(f.window, 550.), button(f.window, false)]);
    f.settle();
    assert_eq!(
        f.app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data
            .as_ref()
            .unwrap(),
        &edited,
        "external bytes must not be overwritten"
    );
    assert!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .history_conflicted(12)
    );
    assert!(!f.save(&f.path.join("conflicted.json")));
}

#[test]
fn shared_layers_v3_invalid_document_open_is_atomic() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.uv(UvLayerCommand::Create {
        name: "Preserved".into(),
    });
    f.direct();
    f.gesture();
    f.settle();
    let path = f.path.join("valid.json");
    assert!(f.save(&path));
    let original = std::fs::read(&path).unwrap();
    let before = f.layers();
    let display = f.image();
    for bad in ["policy", "active", "duplicate"] {
        let mut document: serde_json::Value = serde_json::from_slice(&original).unwrap();
        let uv = document["objects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|o| o["uv_layers"].is_object())
            .unwrap()["uv_layers"]
            .as_object_mut()
            .unwrap();
        match bad {
            "policy" => {
                uv.insert("compositor".into(), serde_json::json!("unknown"));
            }
            "active" => {
                uv.insert("active_layer".into(), serde_json::json!(9999));
            }
            _ => {
                let layers = uv.get_mut("layers").unwrap().as_array_mut().unwrap();
                layers[1]["meta"]["id"] = layers[0]["meta"]["id"].clone();
            }
        }
        let invalid = f.path.join(format!("{bad}.json"));
        std::fs::write(&invalid, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(!f.open(&invalid));
        assert_eq!(f.layers(), before);
        assert_eq!(f.image(), display);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[test]
fn shared_layers_actual_apply_batch_matches_two_separate_commits() {
    let mut batch = Fixture::new_with_projection(true);
    let mut separate = Fixture::new_with_projection(true);
    for f in [&mut batch, &mut separate] {
        f.enable_layers();
        f.command(PaintCommand::SetBrushOpacity { opacity: 0.35 });
        f.gesture();
        f.settle();
    }
    crate::render::dispatch_ui_commands(
        batch.app.world_mut(),
        [
            UiToBevy::PaintCommand(PaintCommand::ProjectToScene),
            UiToBevy::PaintCommand(PaintCommand::ProjectToScene),
        ],
    );
    batch.settle();
    for _ in 0..2 {
        separate.command(PaintCommand::ProjectToScene);
        separate.settle();
    }
    assert_eq!(batch.layers(), separate.layers());
    assert_eq!(counts(&batch), (2, 0));
    assert_eq!(batch.image(), separate.image());
}

#[test]
fn shared_layers_actual_direct_dab_preserves_unpainted_latent_rgb_and_exact_undo() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let path = f.path.join("latent.json");
    assert!(f.save(&path));
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let object = document["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["uv_layers"].is_object())
        .unwrap();
    object["uv_layers"]["layers"][0]["pixels"] = serde_json::json!(vec![[0.8, 0.2, 0.1, 0.]; 4096]);
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    assert!(f.open(&path));
    f.settle();
    let baseline = f.layers();
    f.app.world_mut().resource_mut::<PaintMode>().active = true;
    f.direct();
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 1., 0.5],
    });
    f.gesture();
    f.settle();
    let after = f.layers();
    let pixels = &after.layers[0].pixels;
    assert!(
        pixels.iter().any(|p| *p == [0.8, 0.2, 0.1, 0.]),
        "untouched raw hidden RGB remains exact"
    );
    assert!(
        pixels.iter().any(|p| p[3] > 0.),
        "actual direct dabs accepted"
    );
    for pixel in pixels.iter().filter(|p| p[3] > 0.) {
        assert_eq!(
            pixel[0], 0.,
            "latent red must not ghost into a painted blue texel"
        );
        assert_eq!(pixel[1], 0.);
        assert!(pixel[2] <= pixel[3]);
    }
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), baseline);
    f.uv(UvLayerCommand::Redo);
    assert_eq!(f.layers(), after);
}

#[test]
fn shared_layers_actual_held_canvas_apply_refused_then_cancel_has_no_uv_commit() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let before = f.layers();
    f.batch(vec![
        movement(f.window, 450.),
        button(f.window, true),
        movement(f.window, 550.),
    ]);
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    assert_eq!(f.layers(), before);
    assert_eq!(counts(&f), (0, 0));
    f.command(PaintCommand::CancelStroke);
    f.batch(vec![button(f.window, false)]);
    f.settle();
    assert_eq!(f.layers(), before);
}

#[test]
fn shared_layers_actual_apply_refuses_intervening_source_undo_in_same_dispatch_batch() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.gesture();
    f.settle();
    let source = f.source();
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [
            UiToBevy::PaintCommand(PaintCommand::ProjectToScene),
            UiToBevy::PaintCommand(PaintCommand::Undo),
        ],
    );
    f.settle();
    assert_eq!(
        f.source(),
        source,
        "queued source Undo is explicitly refused while Apply is pending"
    );
    assert_eq!(counts(&f), (1, 0));
    let stamp = f.layers();
    assert!(stamp.layers[0].pixels.iter().any(|p| p[3] > 0.));
    f.command(PaintCommand::Undo);
    f.settle();
    assert_ne!(f.source(), source);
    assert_eq!(f.layers(), stamp);
}

#[test]
fn shared_layers_actual_apply_source_switch_before_commit_refuses_without_wrong_stamp() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.gesture();
    f.settle();
    let before = f.layers();
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [
            UiToBevy::PaintCommand(PaintCommand::ProjectToScene),
            UiToBevy::AddPaintCanvas(pentimento_ipc::AddPaintCanvasRequest {
                width: Some(64),
                height: Some(64),
            }),
        ],
    );
    f.settle();
    assert_ne!(
        f.app.world().resource::<ActiveCanvasPlane>().entity,
        Some(f.canvas)
    );
    assert_eq!(
        f.layers(),
        before,
        "accepted Apply must never stamp a different source"
    );
    assert_eq!(counts(&f), (0, 0));
    assert!(
        f.app
            .world()
            .resource::<OutboundUiMessages>()
            .messages
            .iter()
            .any(|m| matches!(m,BevyToUi::Error{code,..}if code=="uv_projection_rejected"))
    );
}

#[test]
fn shared_layers_actual_color_order_erase_and_display_history() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.direct();
    f.command(PaintCommand::SetBrushColor {
        color: [1., 0., 0., 1.],
    });
    f.command(PaintCommand::SetBrushOpacity { opacity: 0.4 });
    f.gesture();
    f.settle();
    let red = f.layers().layers[0].clone();
    f.uv(UvLayerCommand::Create {
        name: "Blue".into(),
    });
    let blue = f.layers().active_layer;
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 1., 1.],
    });
    f.gesture();
    f.settle();
    let red_under = f.image();
    let before = f.layers();
    f.uv(UvLayerCommand::Reorder {
        layer_id: blue,
        new_index: 0,
    });
    let blue_under = f.image();
    assert_ne!(
        red_under, blue_under,
        "layer order changes actual CPU appearance"
    );
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), before);
    assert_eq!(f.image(), red_under);
    f.uv(UvLayerCommand::Redo);
    assert_eq!(f.image(), blue_under);
    let before_erase = f.layers();
    f.command(PaintCommand::SetBlendMode {
        mode: pentimento_ipc::BlendMode::Erase,
    });
    f.gesture();
    f.settle();
    assert_eq!(
        f.layers().layers[1],
        red,
        "erase changes only the selected blue layer"
    );
    assert_ne!(f.layers().layers[0].pixels, before_erase.layers[0].pixels);
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), before_erase);
    assert_eq!(f.image(), blue_under);
}

#[test]
fn shared_layers_actual_two_receiver_global_history_limit_and_new_commit_redo() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    for i in 0..80 {
        f.uv(UvLayerCommand::Rename {
            layer_id: 0,
            name: format!("First {i}"),
        });
    }
    f.uv(UvLayerCommand::SelectReceiver { mesh_id: 44 });
    f.uv(UvLayerCommand::Enable);
    for i in 0..80 {
        f.uv(UvLayerCommand::Rename {
            layer_id: 0,
            name: format!("Second {i}"),
        });
    }
    let r = f.app.world().resource::<MeshPaintingResource>();
    assert_eq!(r.undo_count(12) + r.undo_count(44), 128);
    assert!(r.history_bytes() <= r.history_limit_bytes());
    assert!(r.evicted_history_strokes() > 0);
    f.uv(UvLayerCommand::Undo);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .redo_count(44),
        1
    );
    f.uv(UvLayerCommand::SelectReceiver { mesh_id: 12 });
    f.uv(UvLayerCommand::Rename {
        layer_id: 0,
        name: "Accepted new edit".into(),
    });
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .redo_count(44),
        0,
        "new accepted edit clears global UV redo"
    );
}

#[test]
fn shared_layers_actual_apply_refuses_changed_object_mapping_and_mesh_asset() {
    for asset_change in [false, true] {
        let mut f = Fixture::new_with_projection(true);
        f.enable_layers();
        f.gesture();
        f.settle();
        let before = f.layers();
        f.command(PaintCommand::ProjectToScene);
        if asset_change {
            let h = f.app.world().get::<Mesh3d>(f.a).unwrap().0.clone();
            let mut assets = f.app.world_mut().resource_mut::<Assets<Mesh>>();
            let mesh = assets.get_mut(&h).unwrap();
            let bevy::mesh::VertexAttributeValues::Float32x3(positions) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!()
            };
            positions[0][2] += 0.1;
        } else {
            f.app
                .world_mut()
                .entity_mut(f.a)
                .insert(Selectable { id: "UV12".into() });
            crate::render::dispatch_ui_commands(
                f.app.world_mut(),
                [UiToBevy::ObjectCommand(
                    pentimento_ipc::ObjectCommand::Transform {
                        id: "UV12".into(),
                        transform: pentimento_ipc::Transform3D {
                            position: [0.2, 0., 0.],
                            rotation: [0., 0., 0., 1.],
                            scale: [1., 1., 1.],
                        },
                    },
                )],
            );
        }
        f.settle();
        assert_eq!(f.layers(), before);
        assert_eq!(counts(&f), (0, 0));
    }
}

#[test]
fn shared_layers_actual_apply_uses_saved_source_view_after_orbit_inspection() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [UiToBevy::AddPaintCanvas(
            pentimento_ipc::AddPaintCanvasRequest {
                width: Some(64),
                height: Some(64),
            },
        )],
    );
    f.settle();
    f.canvas = f
        .app
        .world()
        .resource::<ActiveCanvasPlane>()
        .entity
        .unwrap();
    f.gesture();
    f.settle();
    assert_eq!(
        f.app
            .world()
            .get::<CanvasPlane>(f.canvas)
            .unwrap()
            .paint_camera_pos,
        Some(Vec3::new(0., 0., 4.))
    );
    let camera = f
        .app
        .world_mut()
        .query_filtered::<Entity, With<MainCamera>>()
        .single(f.app.world())
        .unwrap();
    f.app
        .world_mut()
        .get_mut::<Transform>(camera)
        .unwrap()
        .translation = Vec3::new(2., 0., 4.);
    f.settle();
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    assert_eq!(counts(&f), (1, 0));
    assert!(f.layers().layers[0].pixels.iter().any(|p| p[3] > 0.));
}

#[test]
fn shared_layers_actual_pending_apply_open_clears_old_receipts_and_payload() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.gesture();
    f.settle();
    let saved = f.layers();
    let path = f.path.join("pending-open.json");
    assert!(f.save(&path));
    f.command(PaintCommand::ProjectToScene);
    crate::render::dispatch_ui_commands(f.app.world_mut(), [UiToBevy::RequestBrushState]);
    assert!(f.app.world().resource::<OutboundUiMessages>().messages.iter().any(|m|matches!(m,BevyToUi::PaintBrushStateChanged {target,..}if target.pending_bytes>0 && target.active)));
    assert!(f.open(&path));
    f.settle();
    assert_eq!(f.layers(), saved);
    assert_eq!(counts(&f), (0, 0));
    assert!(
        !f.app
            .world()
            .resource::<OutboundUiMessages>()
            .messages
            .iter()
            .any(|m| matches!(m,BevyToUi::Error{code,..}if code=="uv_projection_rejected"))
    );
    crate::render::dispatch_ui_commands(f.app.world_mut(), [UiToBevy::RequestBrushState]);
    assert!(f.app.world().resource::<OutboundUiMessages>().messages.iter().any(|m|matches!(m,BevyToUi::PaintBrushStateChanged {target,..}if target.pending_bytes==0 && !target.active)));
}

#[test]
fn shared_layers_actual_legacy_projection_migration_retains_raw_snapshots() {
    let mut f = Fixture::new_with_projection(true);
    f.command(PaintCommand::SetBrushColor {
        color: [1., 0., 0., 1.],
    });
    f.command(PaintCommand::SetBrushOpacity { opacity: 0.4 });
    f.gesture();
    f.settle();
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    let legacy = f.path.join("legacy-projection.json");
    assert!(f.save(&legacy));
    let serialized: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&legacy).unwrap()).unwrap();
    let object = serialized["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "UV12")
        .unwrap();
    let raw: Vec<(u32, Vec<[f32; 4]>)> =
        serde_json::from_value(object["projection"]["layers"].clone()).unwrap();
    let projected = f
        .app
        .world()
        .resource::<ProjectionTargets>()
        .get(f.a)
        .unwrap()
        .surface()
        .surface()
        .pixels()
        .to_vec();
    assert!(projected.iter().any(|p| p[3] > 0.));
    f.enable_layers();
    let document = f.layers();
    assert_eq!(document.layers.len(), 2);
    assert_eq!(document.layers[0].meta.name, "Canvas 7 snapshot");
    // Canonical v2 stores raw per-canvas associated pixels before legacy display compositing.
    assert_eq!(document.layers[0].pixels, raw[0].1);
    assert_eq!(document.layers[1].pixels, vec![[0.; 4]; 4096]);
    let path = f.path.join("migrated.json");
    assert!(f.save(&path));
    assert!(f.open(&path));
    f.settle();
    assert_eq!(f.layers(), document);
}

#[test]
fn shared_layers_actual_migration_refuses_external_original_direct_and_projection_images() {
    for owner in ["original", "direct", "projection"] {
        let mut f = Fixture::new_with_projection(true);
        f.gesture();
        f.settle();
        f.command(PaintCommand::ProjectToScene);
        f.settle();
        let texture = f.app.world().get::<MeshPaintTexture>(f.a).unwrap();
        let handle = match owner {
            "original" => texture.original_texture.clone().unwrap(),
            "direct" => texture.image_handle.clone(),
            _ => f
                .app
                .world()
                .resource::<Assets<StandardMaterial>>()
                .get(
                    &f.app
                        .world()
                        .get::<MeshMaterial3d<StandardMaterial>>(f.a)
                        .unwrap()
                        .0,
                )
                .unwrap()
                .base_color_texture
                .clone()
                .unwrap(),
        };
        let changed = {
            let mut images = f.app.world_mut().resource_mut::<Assets<Image>>();
            let data = images.get_mut(&handle).unwrap().data.as_mut().unwrap();
            data[0] ^= 127;
            data.clone()
        };
        f.uv(UvLayerCommand::SelectReceiver { mesh_id: 12 });
        f.uv(UvLayerCommand::Enable);
        assert!(
            f.app
                .world()
                .resource::<MeshPaintingResource>()
                .uv_layers(12)
                .is_none(),
            "{owner} external image must refuse migration"
        );
        assert_eq!(
            f.app
                .world()
                .resource::<Assets<Image>>()
                .get(&handle)
                .unwrap()
                .data
                .as_ref()
                .unwrap(),
            &changed,
            "refusal must retain external {owner} bytes"
        );
    }
}

#[test]
fn shared_layers_actual_migration_keeps_both_legacy_direct_and_projection_authoring() {
    let mut f = Fixture::new_with_projection(true);
    f.direct();
    f.gesture();
    f.settle();
    let direct = f.raw();
    f.command(PaintCommand::SetTarget {
        target: PaintTarget::Canvas,
    });
    f.settle();
    f.command(PaintCommand::SetBrushColor {
        color: [1., 0., 0., 1.],
    });
    f.command(PaintCommand::SetBrushOpacity { opacity: 0.4 });
    f.gesture();
    f.settle();
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    let raw = f
        .app
        .world()
        .resource::<ProjectionTargets>()
        .projection_layer_pixels(7, f.a)
        .unwrap()
        .to_vec();
    f.enable_layers();
    let doc = f.layers();
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.layers[1].pixels, direct);
    assert!(doc.layers[0].pixels.iter().any(|p| p[3] > 0.));
    assert_eq!(
        doc.layers[0].pixels, raw,
        "mixed migration preserves actual raw per-canvas snapshots bit for bit"
    );
    let path = f.path.join("migrated-mixed.json");
    assert!(f.save(&path));
    assert!(f.open(&path));
    f.settle();
    assert_eq!(f.layers(), doc);
}

#[test]
fn shared_layers_actual_migration_refuses_duplicate_and_replaced_owner_identities() {
    for replacement in ["duplicate", "mesh", "display"] {
        let mut f = Fixture::new_with_projection(true);
        f.gesture();
        f.settle();
        f.command(PaintCommand::ProjectToScene);
        f.settle();
        f.uv(UvLayerCommand::SelectReceiver { mesh_id: 12 });
        match replacement {
            "duplicate" => {
                f.app
                    .world_mut()
                    .get_mut::<PaintableMesh>(f.b)
                    .unwrap()
                    .mesh_id = 12;
            }
            "mesh" => {
                let h = f
                    .app
                    .world_mut()
                    .resource_mut::<Assets<Mesh>>()
                    .add(Rectangle::new(2., 2.));
                f.app.world_mut().entity_mut(f.a).insert(Mesh3d(h));
            }
            _ => {
                let h = f
                    .app
                    .world()
                    .get::<MeshPaintTexture>(f.a)
                    .unwrap()
                    .image_handle
                    .clone();
                let image = f
                    .app
                    .world()
                    .resource::<Assets<Image>>()
                    .get(&h)
                    .unwrap()
                    .clone();
                let h = f.app.world_mut().resource_mut::<Assets<Image>>().add(image);
                f.app
                    .world_mut()
                    .get_mut::<MeshPaintTexture>(f.a)
                    .unwrap()
                    .image_handle = h;
            }
        }
        f.uv(UvLayerCommand::Enable);
        assert!(
            f.app
                .world()
                .resource::<MeshPaintingResource>()
                .uv_layers(12)
                .is_none(),
            "{replacement} must refuse migration"
        );
    }
}

#[test]
fn shared_layers_actual_migration_refuses_foreign_projection_texel_after_partial_upload() {
    let mut f = Fixture::new_with_projection(true);
    f.gesture();
    f.settle();
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    let h = f
        .app
        .world()
        .resource::<ProjectionTargets>()
        .get_texture(f.a)
        .unwrap()
        .clone();
    f.app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .get_mut(&h)
        .unwrap()
        .data
        .as_mut()
        .unwrap()[0] ^= 127;
    f.command(PaintCommand::SetBrushColor {
        color: [1., 0., 0., 1.],
    });
    f.batch(vec![
        movement(f.window, 500.),
        button(f.window, true),
        movement(f.window, 520.),
        button(f.window, false),
    ]);
    f.settle();
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    f.uv(UvLayerCommand::SelectReceiver { mesh_id: 12 });
    f.uv(UvLayerCommand::Enable);
    assert!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .uv_layers(12)
            .is_none(),
        "partial upload must never acknowledge foreign pixels for migration"
    );
}

#[test]
fn shared_layers_actual_v3_preserves_selected_unlayered_receiver() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.uv(UvLayerCommand::SelectReceiver { mesh_id: 44 });
    let path = f.path.join("selected-unlayered.json");
    assert!(f.save(&path));
    assert!(f.open(&path));
    f.settle();
    assert_eq!(
        f.app.world().resource::<PaintMode>().direct_target,
        Some(f.b)
    );
    assert!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .uv_layers(44)
            .is_none()
    );
}

#[test]
fn shared_live_actual_preview_repeated_strokes_source_history_apply_once() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let baseline = f.layers();
    let before_image = f.image();
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    assert!(f.app.world().resource::<ProjectionMode>().live_projection);
    assert_eq!(counts(&f), (0, 0));
    f.gesture();
    f.settle();
    let one = f.raw();
    let one_image = f.image();
    assert_ne!(one_image, before_image);
    assert_eq!(f.layers(), baseline);
    assert_eq!(counts(&f), (0, 0));
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 1., 1.],
    });
    f.gesture();
    f.settle();
    let two = f.raw();
    assert_ne!(two, one);
    assert_eq!(f.layers(), baseline);
    f.command(PaintCommand::Undo);
    f.settle();
    assert_eq!(f.raw(), one);
    f.command(PaintCommand::Redo);
    f.settle();
    assert_eq!(f.raw(), two);
    let source = f.source();
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    assert_eq!(f.layers().layers[0].pixels, two);
    assert_eq!(counts(&f), (1, 0));
    assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
    f.settle();
    assert_eq!(counts(&f), (1, 0));
    assert_eq!(f.raw(), two);
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), baseline);
    assert_eq!(f.source(), source);
    f.uv(UvLayerCommand::Redo);
    assert_eq!(f.raw(), two);
}
#[test]
fn shared_live_actual_layer_and_target_switches_refuse_until_cancel() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let base = f.layers().active_layer;
    f.uv(UvLayerCommand::Create {
        name: "Live target".into(),
    });
    let before = f.layers();
    let history = counts(&f);
    f.gesture();
    f.settle();
    let source = f.source();
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    assert_ne!(f.raw(), before.layers[1].pixels);
    for op in [
        UvLayerCommand::Select { layer_id: base },
        UvLayerCommand::Lock {
            layer_id: before.active_layer,
            locked: true,
        },
        UvLayerCommand::Visible {
            layer_id: before.active_layer,
            visible: false,
        },
        UvLayerCommand::SelectReceiver { mesh_id: 44 },
        UvLayerCommand::Undo,
    ] {
        f.uv(op);
        assert_eq!(f.layers(), before);
        assert_eq!(counts(&f), history);
    }
    f.command(PaintCommand::SetTarget {
        target: PaintTarget::DirectUv,
    });
    f.settle();
    assert_eq!(
        f.app.world().resource::<PaintMode>().target,
        PaintTarget::Canvas
    );
    f.command(PaintCommand::CancelUvProjection);
    f.settle();
    assert_eq!(f.layers(), before);
    assert_eq!(f.raw(), before.layers[1].pixels);
    assert_eq!(f.source(), source);
    assert_eq!(counts(&f), history);
    f.uv(UvLayerCommand::Select { layer_id: base });
    f.direct();
    f.gesture();
    f.settle();
    assert_ne!(f.layers().layers[0].pixels, before.layers[0].pixels);
    assert_eq!(f.layers().layers[1], before.layers[1]);
}
#[test]
fn shared_live_actual_lock_hidden_cancel_noop_preserve_redo() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let id = f.layers().active_layer;
    for hidden in [false, true] {
        f.uv(if hidden {
            UvLayerCommand::Visible {
                layer_id: id,
                visible: false,
            }
        } else {
            UvLayerCommand::Lock {
                layer_id: id,
                locked: true,
            }
        });
        let before = f.layers();
        let c = counts(&f);
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
        assert_eq!(f.layers(), before);
        assert_eq!(counts(&f), c);
        f.uv(UvLayerCommand::Undo);
    }
    let c = counts(&f);
    assert!(c.1 > 0);
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    assert_eq!(counts(&f), c, "empty preview is a no-op");
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    f.gesture();
    f.settle();
    f.command(PaintCommand::CancelUvProjection);
    f.settle();
    assert_eq!(counts(&f), c);
}
#[test]
fn shared_live_actual_source_stroke_cancel_and_apply_admission() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    let before = f.source();
    let raw = f.raw();
    let doc = f.layers();
    f.batch(vec![movement(f.window, 500.), button(f.window, true)]);
    f.settle();
    assert_ne!(f.raw(), raw);
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    assert_eq!(f.layers(), doc);
    assert_eq!(counts(&f), (0, 0));
    f.command(PaintCommand::CancelStroke);
    f.settle();
    assert_eq!(f.source(), before);
    assert_eq!(f.raw(), raw);
    assert!(f.app.world().resource::<ProjectionMode>().live_projection);
    f.batch(vec![button(f.window, false)]);
    f.command(PaintCommand::CancelUvProjection);
    f.settle();
    assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
}
#[test]
fn shared_live_actual_save_refusal_apply_persistence_open_discards_preview() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    let path = f.path.join("live-v3.json");
    assert!(f.save(&path));
    let baseline = f.layers();
    f.gesture();
    f.settle();
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    let bytes = std::fs::read(&path).unwrap();
    assert!(!f.save(&path));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    f.command(PaintCommand::ProjectToScene);
    f.settle();
    let accepted = f.layers();
    assert_ne!(accepted, baseline);
    assert!(f.save(&path));
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    assert!(f.raw() != accepted.layers[0].pixels);
    assert!(f.open(&path));
    f.settle();
    assert_eq!(f.layers(), accepted);
    assert_eq!(counts(&f), (0, 0));
    assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
    crate::render::dispatch_ui_commands(f.app.world_mut(), [UiToBevy::RequestBrushState]);
    assert!(f.app.world().resource::<OutboundUiMessages>().messages.iter().any(|m|matches!(m,BevyToUi::PaintBrushStateChanged{target,..}if target.pending_bytes==0 && target.uv_layers.as_ref().is_some_and(|s|!s.projection_preview))));
}
#[test]
fn shared_live_actual_external_image_and_mapping_changes_abort_without_commit() {
    for (external, cancel_now) in [(true, false), (true, true), (false, false)] {
        let mut f = Fixture::new_with_projection(true);
        f.enable_layers();
        f.gesture();
        f.settle();
        let doc = f.layers();
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        let h = f
            .app
            .world()
            .get::<MeshPaintTexture>(f.a)
            .unwrap()
            .image_handle
            .clone();
        let foreign = if external {
            let mut images = f.app.world_mut().resource_mut::<Assets<Image>>();
            let bytes = images.get_mut(&h).unwrap().data.as_mut().unwrap();
            bytes[0] ^= 127;
            Some(bytes.clone())
        } else {
            f.app
                .world_mut()
                .get_mut::<Transform>(f.a)
                .unwrap()
                .translation
                .x += 0.2;
            None
        };
        if cancel_now {
            f.command(PaintCommand::CancelUvProjection);
        }
        f.settle();
        assert_eq!(f.layers(), doc);
        assert_eq!(counts(&f), (0, 0));
        assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
        if let Some(bytes) = foreign {
            assert_eq!(
                f.app
                    .world()
                    .resource::<Assets<Image>>()
                    .get(&h)
                    .unwrap()
                    .data
                    .as_ref()
                    .unwrap(),
                &bytes
            );
            assert!(
                f.app
                    .world()
                    .resource::<MeshPaintingResource>()
                    .history_conflicted(12)
            );
        } else {
            assert_eq!(f.raw(), doc.layers[0].pixels);
        }
    }
}
#[test]
fn shared_live_actual_mode_exit_and_source_switch_release_preview() {
    for reason in 0..3 {
        let mut f = Fixture::new_with_projection(true);
        f.enable_layers();
        f.gesture();
        f.settle();
        let doc = f.layers();
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        if reason == 1 {
            f.app.world_mut().resource_mut::<ActiveCanvasPlane>().entity = None;
        } else if reason == 0 {
            f.app.world_mut().resource_mut::<PaintMode>().active = false;
        } else {
            f.batch(vec![WindowEvent::WindowFocused(
                bevy::window::WindowFocused {
                    window: f.window,
                    focused: false,
                },
            )]);
        }
        f.settle();
        assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
        assert_eq!(f.layers(), doc);
        assert_eq!(f.raw(), doc.layers[0].pixels);
        assert_eq!(counts(&f), (0, 0));
    }
}
#[test]
fn shared_live_actual_default_resolution_fits_accounted_pending_limits() {
    let mut f = Fixture::new_projection_sized(true, 1024);
    f.enable_layers();
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    assert!(f.app.world().resource::<ProjectionMode>().live_projection);
    crate::render::dispatch_ui_commands(f.app.world_mut(), [UiToBevy::RequestBrushState]);
    let out = f.app.world().resource::<OutboundUiMessages>();
    let target = out
        .messages
        .iter()
        .rev()
        .find_map(|m| {
            if let BevyToUi::PaintBrushStateChanged { target, .. } = m {
                Some(target)
            } else {
                None
            }
        })
        .unwrap();
    assert!(target.pending_bytes >= 32 * 1024 * 1024);
    assert!(target.pending_bytes <= 64 * 1024 * 1024);
    assert!(!target.active);
    assert!(target.uv_layers.as_ref().unwrap().projection_preview);
    f.command(PaintCommand::CancelUvProjection);
    f.settle();
    assert_eq!(counts(&f), (0, 0));
}

#[test]
fn shared_live_actual_apply_then_cancel_same_batch_preserves_redo() {
    for disable in [false, true] {
        let mut f = Fixture::new_with_projection(true);
        f.enable_layers();
        f.uv(UvLayerCommand::Create {
            name: "Redo".into(),
        });
        f.uv(UvLayerCommand::Undo);
        let c = counts(&f);
        let before = f.layers();
        f.gesture();
        f.settle();
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        let source = f.source();
        crate::render::dispatch_ui_commands(
            f.app.world_mut(),
            [
                UiToBevy::PaintCommand(PaintCommand::ProjectToScene),
                UiToBevy::PaintCommand(if disable {
                    PaintCommand::SetLiveProjection { enabled: false }
                } else {
                    PaintCommand::CancelUvProjection
                }),
            ],
        );
        f.settle();
        assert_eq!(f.layers(), before);
        assert_eq!(counts(&f), c);
        assert_eq!(f.source(), source);
        assert_eq!(f.raw(), before.layers[0].pixels);
        assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
    }
}
#[test]
fn shared_live_actual_cancel_without_preview_keeps_legacy_projection() {
    let mut f = Fixture::new_with_projection(true);
    f.gesture();
    f.settle();
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    let image = f.image();
    f.command(PaintCommand::CancelUvProjection);
    f.settle();
    assert!(f.app.world().resource::<ProjectionMode>().live_projection);
    assert_eq!(f.image(), image);
}

#[test]
fn shared_live_actual_late_frame_foreign_image_or_material_refuses_apply() {
    for image_edit in [true, false] {
        let mut f = Fixture::new_with_projection(true);
        f.enable_layers();
        f.gesture();
        f.settle();
        let doc = f.layers();
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        let h = f
            .app
            .world()
            .get::<MeshPaintTexture>(f.a)
            .unwrap()
            .image_handle
            .clone();
        let m = f
            .app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(f.a)
            .unwrap()
            .0
            .clone();
        let mut expected = f.image();
        expected[0] ^= 127;
        // Deterministically mutate after every Update owner/upload system and
        // before the PostUpdate transform/projection chain commits Apply.
        let handle = h.clone();
        let mut done = false;
        f.app.add_systems(
            PostUpdate,
            (move |world: &mut World| {
                if done {
                    return;
                }
                done = true;
                if image_edit {
                    world
                        .resource_mut::<Assets<Image>>()
                        .get_mut(&handle)
                        .unwrap()
                        .data
                        .as_mut()
                        .unwrap()[0] ^= 127;
                } else {
                    world
                        .resource_mut::<Assets<StandardMaterial>>()
                        .get_mut(&m)
                        .unwrap()
                        .base_color = Color::srgb(1., 0., 0.);
                }
            })
            .before(bevy::transform::TransformSystems::Propagate),
        );
        f.command(PaintCommand::ProjectToScene);
        f.settle();
        assert_eq!(f.layers(), doc);
        assert_eq!(counts(&f), (0, 0));
        assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
        assert!(
            f.app
                .world()
                .resource::<MeshPaintingResource>()
                .history_conflicted(12)
        );
        if image_edit {
            assert_eq!(
                f.app
                    .world()
                    .resource::<Assets<Image>>()
                    .get(&h)
                    .unwrap()
                    .data
                    .as_ref()
                    .unwrap(),
                &expected
            );
        } else {
            let material = f
                .app
                .world()
                .get::<MeshMaterial3d<StandardMaterial>>(f.a)
                .unwrap();
            assert_eq!(
                f.app
                    .world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(&material.0)
                    .unwrap()
                    .base_color,
                Color::srgb(1., 0., 0.)
            );
        }
    }
}

#[test]
fn shared_live_actual_two_focus_cycles_are_consumed_before_new_preview() {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    let doc = f.layers();
    let event = |focused| {
        WindowEvent::WindowFocused(bevy::window::WindowFocused {
            window: f.window,
            focused,
        })
    };
    f.batch(vec![event(false), event(true), event(false), event(true)]);
    assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
    assert_eq!(f.layers(), doc);
    assert_eq!(counts(&f), (0, 0));
    f.command(PaintCommand::SetLiveProjection { enabled: true });
    f.settle();
    assert!(f.app.world().resource::<ProjectionMode>().live_projection);
    f.gesture();
    f.settle();
    assert_ne!(f.raw(), doc.layers[0].pixels);
    assert_eq!(f.layers(), doc);
    assert_eq!(counts(&f), (0, 0));
    f.command(PaintCommand::CancelUvProjection);
    f.settle();
}

fn mask_fixture() -> Fixture {
    let mut f = Fixture::new_with_projection(true);
    f.enable_layers();
    f.direct();
    f.command(PaintCommand::SetBrushColor {
        color: [0.8, 0.3, 0.5, 1.],
    });
    f.gesture();
    f.settle();
    f.uv(UvLayerCommand::AddMask { layer_id: 0 });
    f.uv(UvLayerCommand::PaintTarget {
        layer_id: 0,
        target: pentimento_ipc::UvLayerPaintTarget::Mask,
    });
    f
}

#[test]
fn shared_masks_actual_direct_strokes_scalar_history_erase_cancel_noop_and_reopen() {
    let mut f = mask_fixture();
    let baseline = f.layers();
    let image = f.image();
    let n = counts(&f).0;
    let bytes = f
        .app
        .world()
        .resource::<MeshPaintingResource>()
        .history_bytes();
    f.command(PaintCommand::SetBrushColor { color: [1.; 4] });
    f.gesture();
    f.settle();
    assert_eq!(f.layers(), baseline);
    assert_eq!(counts(&f), (n, 0));
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 0., 1.],
    });
    f.gesture();
    f.settle();
    let masked = f.layers();
    let masked_image = f.image();
    assert_eq!(masked.layers[0].pixels, baseline.layers[0].pixels);
    assert!(
        masked.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .iter()
            .any(|&m| m < 1.)
    );
    assert_ne!(masked_image, image);
    assert_eq!(counts(&f), (n + 1, 0));
    let delta = f
        .app
        .world()
        .resource::<MeshPaintingResource>()
        .history_bytes()
        - bytes;
    assert!(
        delta >= 64 * 64 * 8 && delta < 64 * 64 * 16,
        "retains two scalar snapshots, not color snapshots: {delta}"
    );
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), baseline);
    assert_eq!(f.image(), image);
    f.uv(UvLayerCommand::PaintTarget {
        layer_id: 0,
        target: pentimento_ipc::UvLayerPaintTarget::Color,
    });
    assert_eq!(counts(&f), (n, 1));
    f.uv(UvLayerCommand::PaintTarget {
        layer_id: 0,
        target: pentimento_ipc::UvLayerPaintTarget::Mask,
    });
    f.command(PaintCommand::SetBrushOpacity { opacity: 0. });
    f.gesture();
    f.settle();
    assert_eq!(counts(&f), (n, 1));
    assert_eq!(f.layers(), baseline);
    f.command(PaintCommand::SetBrushOpacity { opacity: 1. });
    f.batch(vec![movement(f.window, 500.), button(f.window, true)]);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .pending_history_bytes(),
        64 * 64 * 16
    );
    f.command(PaintCommand::CancelStroke);
    f.batch(vec![button(f.window, false)]);
    f.settle();
    assert_eq!(f.layers(), baseline);
    assert_eq!(counts(&f), (n, 1));
    f.uv(UvLayerCommand::Redo);
    assert_eq!(f.layers(), masked);
    assert_eq!(f.image(), masked_image);
    f.command(PaintCommand::SetBlendMode {
        mode: pentimento_ipc::BlendMode::Erase,
    });
    f.gesture();
    f.settle();
    let erased = f.layers();
    assert_ne!(erased, masked);
    assert_eq!(erased.layers[0].pixels, baseline.layers[0].pixels);
    assert!(
        erased.layers[0]
            .mask
            .as_ref()
            .unwrap()
            .iter()
            .zip(masked.layers[0].mask.as_ref().unwrap())
            .any(|(a, b)| a > b)
    );
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), masked);
    let path = f.path.join("mask-direct.json");
    assert!(f.save(&path));
    assert!(f.open(&path));
    f.settle();
    assert_eq!(f.layers(), masked);
    assert_eq!(f.image(), masked_image);
    assert_eq!(counts(&f), (0, 0));
    f.batch(vec![
        key(f.window, KeyCode::ShiftLeft, true),
        key(f.window, KeyCode::Tab, true),
    ]);
    f.batch(vec![
        key(f.window, KeyCode::Tab, false),
        key(f.window, KeyCode::ShiftLeft, false),
    ]);
    f.direct();
    f.command(PaintCommand::SetBlendMode {
        mode: pentimento_ipc::BlendMode::Erase,
    });
    f.gesture();
    f.settle();
    assert_eq!(counts(&f), (1, 0));
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), masked);
    assert_eq!(f.image(), masked_image);
}

#[test]
fn shared_masks_actual_modes_opacity_enable_order_selection_and_lifecycle() {
    use pentimento_ipc::{UvLayerBlendMode as Mode, UvLayerPaintTarget as Target};
    for mode in [Mode::Normal, Mode::Multiply, Mode::Screen, Mode::Overlay] {
        let mut f = mask_fixture();
        f.uv(UvLayerCommand::PaintTarget {
            layer_id: 0,
            target: Target::Color,
        });
        f.uv(UvLayerCommand::Create {
            name: "Masked top".into(),
        });
        let id = f.layers().active_layer;
        f.command(PaintCommand::SetBrushOpacity { opacity: 0.55 });
        f.command(PaintCommand::SetBrushColor {
            color: [0.2, 0.7, 0.4, 1.],
        });
        f.gesture();
        f.settle();
        f.uv(UvLayerCommand::BlendMode { layer_id: id, mode });
        let colors = f
            .layers()
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>();
        f.uv(UvLayerCommand::AddMask { layer_id: id });
        f.uv(UvLayerCommand::PaintTarget {
            layer_id: id,
            target: Target::Mask,
        });
        f.command(PaintCommand::SetBrushColor {
            color: [0., 0., 0., 1.],
        });
        f.gesture();
        f.settle();
        let before = f.layers();
        let image = f.image();
        f.uv(UvLayerCommand::Opacity {
            layer_id: id,
            opacity: 0.4,
        });
        let faded = f.layers();
        let faded_image = f.image();
        assert_ne!(faded_image, image);
        f.uv(UvLayerCommand::Reorder {
            layer_id: id,
            new_index: 0,
        });
        let reordered = f.layers();
        let reordered_image = f.image();
        assert_ne!(reordered_image, faded_image);
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), faded);
        assert_eq!(f.image(), faded_image);
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), before);
        assert_eq!(f.image(), image);
        f.uv(UvLayerCommand::Select { layer_id: 0 });
        assert_eq!(counts(&f).1, 2);
        f.uv(UvLayerCommand::Redo);
        f.uv(UvLayerCommand::Redo);
        assert_eq!(f.layers(), reordered);
        assert_eq!(f.image(), reordered_image);
        f.uv(UvLayerCommand::MaskEnabled {
            layer_id: id,
            enabled: false,
        });
        let disabled = f.layers();
        assert_ne!(f.image(), reordered_image);
        let selected = disabled.layers.iter().find(|l| l.meta.id == id).unwrap();
        assert_eq!(
            selected.meta.paint_target,
            painting::uv_layers::UvPaintTarget::Color
        );
        f.uv(UvLayerCommand::PaintTarget {
            layer_id: id,
            target: Target::Mask,
        });
        assert_eq!(f.layers(), disabled);
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), reordered);
        assert_eq!(f.image(), reordered_image);
        for (layer, color) in before.layers.iter().zip(&colors) {
            assert_eq!(&layer.pixels, color);
        }
        f.uv(UvLayerCommand::Duplicate { layer_id: id });
        let duplicate = f.layers();
        let copy = duplicate.active_layer;
        assert_eq!(
            duplicate
                .layers
                .iter()
                .find(|l| l.meta.id == copy)
                .unwrap()
                .mask,
            reordered
                .layers
                .iter()
                .find(|l| l.meta.id == id)
                .unwrap()
                .mask
        );
        f.uv(UvLayerCommand::Delete { layer_id: copy });
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), duplicate);
        f.uv(UvLayerCommand::RemoveMask { layer_id: copy });
        assert!(
            f.layers()
                .layers
                .iter()
                .find(|l| l.meta.id == copy)
                .unwrap()
                .mask
                .is_none()
        );
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), duplicate);
        let path = f.path.join(format!("mask-{mode:?}.json"));
        assert!(f.save(&path));
        assert!(f.open(&path));
        f.settle();
        assert_eq!(f.layers(), duplicate);
    }
}

#[test]
fn shared_masks_actual_projection_and_live_recompute_baseline_apply_once_cancel_keep_source() {
    use pentimento_ipc::{UvLayerBlendMode as Mode, UvLayerPaintTarget as Target};
    for mode in [Mode::Normal, Mode::Multiply, Mode::Screen, Mode::Overlay] {
        let mut f = mask_fixture();
        f.uv(UvLayerCommand::BlendMode { layer_id: 0, mode });
        f.command(PaintCommand::SetTarget {
            target: PaintTarget::Canvas,
        });
        f.command(PaintCommand::SetBrushColor {
            color: [0., 0., 0., 1.],
        });
        f.gesture();
        f.settle();
        let before = f.layers();
        let n = counts(&f);
        let image = f.image();
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        let one = f.image();
        let work = f.raw();
        assert_ne!(one, image);
        assert_eq!(f.layers(), before);
        assert_eq!(counts(&f), n);
        f.settle();
        assert_eq!(f.raw(), work, "preview must not compound on itself");
        for op in [
            UvLayerCommand::PaintTarget {
                layer_id: 0,
                target: Target::Color,
            },
            UvLayerCommand::MaskEnabled {
                layer_id: 0,
                enabled: false,
            },
            UvLayerCommand::RemoveMask { layer_id: 0 },
        ] {
            f.uv(op);
            assert_eq!(f.layers(), before);
        }
        f.command(PaintCommand::SetBrushColor {
            color: [0.2, 0.7, 0.4, 1.],
        });
        f.gesture();
        f.settle();
        let two = f.image();
        assert_ne!(two, one);
        f.command(PaintCommand::Undo);
        f.settle();
        assert_eq!(f.image(), one);
        f.command(PaintCommand::Redo);
        f.settle();
        assert_eq!(f.image(), two);
        let source = f.source();
        f.command(PaintCommand::CancelUvProjection);
        f.settle();
        assert_eq!(f.layers(), before);
        assert_eq!(f.image(), image);
        assert_eq!(f.source(), source);
        assert_eq!(counts(&f), n);
        f.command(PaintCommand::ProjectToScene);
        f.settle();
        let applied = f.layers();
        assert_eq!(f.image(), two);
        assert_eq!(applied.layers[0].pixels, before.layers[0].pixels);
        assert_ne!(applied.layers[0].mask, before.layers[0].mask);
        assert_eq!(counts(&f).0, n.0 + 1);
        f.uv(UvLayerCommand::Undo);
        assert_eq!(f.layers(), before);
        assert_eq!(f.source(), source);
        f.command(PaintCommand::SetLiveProjection { enabled: true });
        f.settle();
        f.command(PaintCommand::ProjectToScene);
        f.settle();
        assert_eq!(f.layers(), applied);
        assert_eq!(f.image(), two);
        assert_eq!(counts(&f).1, 0);
        assert!(!f.app.world().resource::<ProjectionMode>().live_projection);
        f.settle();
        assert_eq!(counts(&f).0, n.0 + 1);
    }
}

#[test]
fn shared_masks_actual_held_direct_pending_apply_and_unmasked_target_refuse_channel_changes() {
    use pentimento_ipc::UvLayerPaintTarget as Target;
    let mut f = mask_fixture();
    let before = f.layers();
    let n = counts(&f);
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 0., 1.],
    });
    f.batch(vec![movement(f.window, 500.), button(f.window, true)]);
    f.uv(UvLayerCommand::PaintTarget {
        layer_id: 0,
        target: Target::Color,
    });
    assert_eq!(f.layers(), before);
    f.command(PaintCommand::CancelStroke);
    f.batch(vec![button(f.window, false)]);
    f.settle();
    assert_eq!(f.layers(), before);
    assert_eq!(counts(&f), n);
    f.command(PaintCommand::SetTarget {
        target: PaintTarget::Canvas,
    });
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 0., 1.],
    });
    f.gesture();
    f.settle();
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [
            UiToBevy::PaintCommand(PaintCommand::ProjectToScene),
            UiToBevy::PaintCommand(PaintCommand::UvLayers {
                command: UvLayerCommand::PaintTarget {
                    layer_id: 0,
                    target: Target::Color,
                },
            }),
        ],
    );
    assert_eq!(f.layers(), before);
    f.settle();
    let applied = f.layers();
    assert_eq!(
        applied.layers[0].meta.paint_target,
        painting::uv_layers::UvPaintTarget::Mask
    );
    assert_ne!(applied.layers[0].mask, before.layers[0].mask);
    assert_eq!(applied.layers[0].pixels, before.layers[0].pixels);
    f.uv(UvLayerCommand::Create {
        name: "No mask".into(),
    });
    let clean = f.layers();
    let id = clean.active_layer;
    f.uv(UvLayerCommand::PaintTarget {
        layer_id: id,
        target: Target::Mask,
    });
    assert_eq!(f.layers(), clean);
}

#[test]
fn shared_masks_actual_invalid_v3_open_keeps_document_images_history_source_and_files() {
    let mut f = mask_fixture();
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 0., 1.],
    });
    f.gesture();
    f.settle();
    let path = f.path.join("mask-good.json");
    assert!(f.save(&path));
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let baseline = f.layers();
    let image = f.image();
    let source = f.source();
    let n = counts(&f);
    let good_bytes = std::fs::read(&path).unwrap();
    for kind in [
        "count",
        "range",
        "disabled-target",
        "absent",
        "policy",
        "unknown-target",
    ] {
        let mut invalid = saved.clone();
        let uv = invalid["objects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|o| o["uv_layers"].is_object())
            .unwrap()
            .get_mut("uv_layers")
            .unwrap();
        match kind {
            "count" => {
                uv["layers"][0]["mask"] = serde_json::json!([1.]);
            }
            "range" => {
                uv["layers"][0]["mask"][0] = serde_json::json!(-0.1);
            }
            "disabled-target" => {
                uv["layers"][0]["meta"]["mask_enabled"] = serde_json::json!(false);
            }
            "absent" => {
                uv["layers"][0].as_object_mut().unwrap().remove("mask");
            }
            "policy" => {
                uv["compositor"] = serde_json::json!(painting::uv_layers::UV_BLEND_COMPOSITOR);
            }
            _ => {
                uv["layers"][0]["meta"]["paint_target"] = serde_json::json!("Alpha");
            }
        }
        let bad = f.path.join(format!("mask-bad-{kind}.json"));
        let bytes = serde_json::to_vec(&invalid).unwrap();
        std::fs::write(&bad, &bytes).unwrap();
        assert!(!f.open(&bad), "{kind}");
        assert_eq!(f.layers(), baseline);
        assert_eq!(f.image(), image);
        assert_eq!(f.source(), source);
        assert_eq!(counts(&f), n);
        assert_eq!(std::fs::read(&bad).unwrap(), bytes);
        assert_eq!(std::fs::read(&path).unwrap(), good_bytes);
    }
}

#[test]
fn shared_masks_actual_external_display_conflict_keeps_foreign_pixels_and_refuses_history() {
    let mut f = mask_fixture();
    f.command(PaintCommand::SetBrushColor {
        color: [0., 0., 0., 1.],
    });
    f.gesture();
    f.settle();
    let before = f.layers();
    let handle = f
        .app
        .world()
        .get::<MeshPaintTexture>(f.a)
        .unwrap()
        .image_handle
        .clone();
    let foreign = {
        let mut images = f.app.world_mut().resource_mut::<Assets<Image>>();
        let data = images.get_mut(&handle).unwrap().data.as_mut().unwrap();
        data[0] ^= 127;
        data.clone()
    };
    f.settle();
    f.uv(UvLayerCommand::Undo);
    assert_eq!(f.layers(), before);
    assert!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .history_conflicted(12)
    );
    assert_eq!(
        f.app
            .world()
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data
            .as_ref()
            .unwrap(),
        &foreign
    );
    assert!(!f.save(&f.path.join("mask-conflicted.json")));
}

#[test]
fn shared_masks_actual_mixed_receiver_history_cap_and_global_redo_invalidation() {
    let mut f = mask_fixture();
    for i in 0..80 {
        f.uv(UvLayerCommand::MaskEnabled {
            layer_id: 0,
            enabled: i % 2 == 0,
        });
    }
    f.uv(UvLayerCommand::SelectReceiver { mesh_id: 44 });
    f.uv(UvLayerCommand::Enable);
    f.uv(UvLayerCommand::AddMask { layer_id: 0 });
    for i in 0..80 {
        f.uv(UvLayerCommand::MaskEnabled {
            layer_id: 0,
            enabled: i % 2 == 0,
        });
    }
    let r = f.app.world().resource::<MeshPaintingResource>();
    assert_eq!(r.undo_count(12) + r.undo_count(44), 128);
    assert!(r.evicted_history_strokes() > 0);
    assert!(r.history_bytes() <= r.history_limit_bytes());
    f.uv(UvLayerCommand::Undo);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .redo_count(44),
        1
    );
    f.uv(UvLayerCommand::SelectReceiver { mesh_id: 12 });
    f.uv(UvLayerCommand::PaintTarget {
        layer_id: 0,
        target: pentimento_ipc::UvLayerPaintTarget::Color,
    });
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .redo_count(44),
        1
    );
    f.uv(UvLayerCommand::MaskEnabled {
        layer_id: 0,
        enabled: true,
    });
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .redo_count(44),
        0
    );
}

#[test]
fn shared_masks_actual_untouched_scalar_bits_and_disabled_mask_survive_owned_reopen() {
    let mut f = mask_fixture();
    let path = f.path.join("mask-bits.json");
    assert!(f.save(&path));
    let mut saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let uv = saved["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["uv_layers"].is_object())
        .unwrap()
        .get_mut("uv_layers")
        .unwrap();
    let exact = [-0., f32::from_bits(1), f32::from_bits(0x3eaaaaab)];
    for (i, m) in exact.iter().enumerate() {
        uv["layers"][0]["mask"][i] = serde_json::to_value(m).unwrap();
    }
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(f.open(&path));
    f.settle();
    let baseline = f.layers();
    let raw_color = baseline.layers[0].pixels.clone();
    for (i, m) in exact.iter().enumerate() {
        assert_eq!(
            baseline.layers[0].mask.as_ref().unwrap()[i].to_bits(),
            m.to_bits()
        );
    }
    f.batch(vec![
        key(f.window, KeyCode::ShiftLeft, true),
        key(f.window, KeyCode::Tab, true),
    ]);
    f.batch(vec![
        key(f.window, KeyCode::Tab, false),
        key(f.window, KeyCode::ShiftLeft, false),
    ]);
    f.direct();
    f.command(PaintCommand::SetBrushColor {
        color: [0.17, 0.17, 0.17, 1.],
    });
    f.gesture();
    f.settle();
    let painted = f.layers();
    assert_ne!(painted.layers[0].mask, baseline.layers[0].mask);
    for (i, m) in exact.iter().enumerate() {
        assert_eq!(
            painted.layers[0].mask.as_ref().unwrap()[i].to_bits(),
            m.to_bits()
        );
    }
    assert!(painting::uv_layers::same_uv_pixels(
        &painted.layers[0].pixels,
        &raw_color
    ));
    f.uv(UvLayerCommand::Undo);
    assert!(painting::uv_layers::same_mask_pixels(
        f.layers().layers[0].mask.as_ref().unwrap(),
        baseline.layers[0].mask.as_ref().unwrap()
    ));
    f.uv(UvLayerCommand::Redo);
    f.uv(UvLayerCommand::MaskEnabled {
        layer_id: 0,
        enabled: false,
    });
    let disabled = f.layers();
    let image = f.image();
    let disabled_path = f.path.join("mask-disabled.json");
    assert!(f.save(&disabled_path));
    assert!(f.open(&disabled_path));
    f.settle();
    let reopened = f.layers();
    assert_eq!(
        reopened.layers[0].meta.paint_target,
        painting::uv_layers::UvPaintTarget::Color
    );
    assert!(!reopened.layers[0].meta.mask_enabled);
    assert!(painting::uv_layers::same_mask_pixels(
        reopened.layers[0].mask.as_ref().unwrap(),
        disabled.layers[0].mask.as_ref().unwrap()
    ));
    assert!(painting::uv_layers::same_uv_pixels(
        &reopened.layers[0].pixels,
        &raw_color
    ));
    assert_eq!(f.image(), image);
    assert_eq!(counts(&f), (0, 0));
}

// Real egui widgets -> the existing shared dispatcher -> production CPU assets.
fn egui_snapshot(f: &mut Fixture) -> pentimento_egui_ui::EguiUiSnapshot {
    crate::render::dispatch_ui_commands(f.app.world_mut(), [UiToBevy::RequestBrushState]);
    let mut snapshot = pentimento_egui_ui::EguiUiSnapshot::default();
    for message in f
        .app
        .world_mut()
        .resource_mut::<OutboundUiMessages>()
        .drain()
    {
        pentimento_egui_ui::apply_bevy_message(&mut snapshot, message);
    }
    snapshot
}
fn egui_frame(
    context: &pentimento_egui_ui::egui::Context,
    snapshot: &mut pentimento_egui_ui::EguiUiSnapshot,
    runtime: &mut pentimento_egui_ui::EguiUiRuntime,
    events: Vec<pentimento_egui_ui::egui::Event>,
) -> (pentimento_egui_ui::egui::FullOutput, Vec<UiToBevy>) {
    use pentimento_egui_ui::egui;
    let mut commands = Vec::new();
    let output = context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 1800.),
            )),
            events,
            ..default()
        },
        |ctx| commands.extend(pentimento_egui_ui::show_root_ui(ctx, snapshot, runtime)),
    );
    (output, commands)
}
fn egui_click(
    f: &mut Fixture,
    context: &pentimento_egui_ui::egui::Context,
    snapshot: &mut pentimento_egui_ui::EguiUiSnapshot,
    runtime: &mut pentimento_egui_ui::EguiUiRuntime,
    label: &str,
) {
    use pentimento_egui_ui::egui;
    egui_frame(context, snapshot, runtime, vec![]);
    let (output, _) = egui_frame(context, snapshot, runtime, vec![]);
    fn position(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() / 2.)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| position(s, label)),
            _ => None,
        }
    }
    let pos = output
        .shapes
        .iter()
        .find_map(|s| position(&s.shape, label))
        .unwrap_or_else(|| panic!("missing egui widget: {label}"));
    egui_frame(
        context,
        snapshot,
        runtime,
        vec![egui::Event::PointerMoved(pos)],
    );
    egui_frame(
        context,
        snapshot,
        runtime,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: default(),
        }],
    );
    let (_, commands) = egui_frame(
        context,
        snapshot,
        runtime,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: default(),
        }],
    );
    crate::render::dispatch_ui_commands(f.app.world_mut(), commands);
    f.settle();
}
#[test]
fn egui_widgets_restore_real_directuv_pixels_and_redo_via_shared_backend() {
    let mut f = Fixture::new();
    f.app
        .world_mut()
        .resource_mut::<crate::config::PentimentoConfig>()
        .composite_mode = crate::config::CompositeMode::Egui;
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    let before = f.raw();
    pen_stroke(&mut f, 91, 0.7);
    let accepted = f.raw();
    assert!(!same(&before, &accepted));
    let ctx = pentimento_egui_ui::egui::Context::default();
    ctx.style_mut(|s| s.animation_time = 0.);
    let mut runtime = pentimento_egui_ui::EguiUiRuntime::default();
    let mut snapshot = egui_snapshot(&mut f);
    egui_click(
        &mut f,
        &ctx,
        &mut snapshot,
        &mut runtime,
        "Undo paint stroke",
    );
    assert!(same(&before, &f.raw()));
    assert_eq!(counts(&f), (0, 1));
    snapshot = egui_snapshot(&mut f);
    egui_click(
        &mut f,
        &ctx,
        &mut snapshot,
        &mut runtime,
        "Redo paint stroke",
    );
    assert!(same(&accepted, &f.raw()));
    assert_eq!(counts(&f), (1, 0));
}
#[test]
fn egui_actual_panel_regions_capture_stationary_press_and_drag_origin() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    f.app
        .world_mut()
        .resource_mut::<crate::config::PentimentoConfig>()
        .composite_mode = crate::config::CompositeMode::Egui;
    let ctx = pentimento_egui_ui::egui::Context::default();
    let mut runtime = pentimento_egui_ui::EguiUiRuntime::default();
    let mut snapshot = egui_snapshot(&mut f);
    egui_frame(&ctx, &mut snapshot, &mut runtime, vec![]);
    let regions = runtime
        .ui_regions()
        .iter()
        .enumerate()
        .map(|(i, r)| pentimento_ipc::LayoutRegion {
            id: format!("egui-{i}"),
            x: r.min.x,
            y: r.min.y,
            width: r.width(),
            height: r.height(),
            z_index: i as i32,
            accepts_keyboard: true,
        })
        .collect();
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [UiToBevy::LayoutUpdate(pentimento_ipc::LayoutInfo {
            regions,
        })],
    );
    let before = f.raw();
    let w = f.window;
    f.batch(vec![movement(w, 700.)]);
    f.batch(vec![button(w, true), movement(w, 450.), button(w, false)]);
    f.settle();
    assert!(same(&before, &f.raw()));
    assert_eq!(counts(&f), (0, 0));
    pen_stroke(&mut f, 92, 0.7);
    assert!(!same(&before, &f.raw()));
    assert_eq!(counts(&f), (1, 0));
}
#[test]
fn egui_widgets_select_and_remove_the_real_uv_mask() {
    let mut f = Fixture::new();
    f.enable_layers();
    let layer = f.layers().layers[0].meta.id;
    f.uv(UvLayerCommand::AddMask { layer_id: layer });
    let ctx = pentimento_egui_ui::egui::Context::default();
    ctx.style_mut(|s| s.animation_time = 0.);
    let mut runtime = pentimento_egui_ui::EguiUiRuntime::default();
    let mut snapshot = egui_snapshot(&mut f);
    egui_click(&mut f, &ctx, &mut snapshot, &mut runtime, "Paint mask");
    assert_eq!(
        f.layers().layers[0].meta.paint_target,
        painting::uv_layers::UvPaintTarget::Mask
    );
    snapshot = egui_snapshot(&mut f);
    egui_click(&mut f, &ctx, &mut snapshot, &mut runtime, "Remove mask");
    assert!(f.layers().layers[0].mask.is_none());
}
#[test]
fn egui_actual_file_popup_captures_a_stationary_native_press() {
    let mut f = Fixture::new();
    f.direct();
    f.app
        .world_mut()
        .resource_mut::<crate::config::PentimentoConfig>()
        .composite_mode = crate::config::CompositeMode::Egui;
    let ctx = pentimento_egui_ui::egui::Context::default();
    ctx.style_mut(|s| s.animation_time = 0.);
    let mut runtime = pentimento_egui_ui::EguiUiRuntime::default();
    let mut snapshot = egui_snapshot(&mut f);
    egui_click(&mut f, &ctx, &mut snapshot, &mut runtime, "File");
    egui_frame(&ctx, &mut snapshot, &mut runtime, vec![]);
    let popup = *runtime
        .ui_regions()
        .iter()
        .find(|r| r.max.y > 44. && r.max.x < 660. && r.height() > 40.)
        .expect("actual File popup rectangle");
    let point = popup.center();
    assert!(point.y > 44.);
    let regions = runtime
        .ui_regions()
        .iter()
        .enumerate()
        .map(|(i, r)| pentimento_ipc::LayoutRegion {
            id: format!("egui-{i}"),
            x: r.min.x,
            y: r.min.y,
            width: r.width(),
            height: r.height(),
            z_index: i as i32,
            accepts_keyboard: true,
        })
        .collect();
    crate::render::dispatch_ui_commands(
        f.app.world_mut(),
        [UiToBevy::LayoutUpdate(pentimento_ipc::LayoutInfo {
            regions,
        })],
    );
    let w = f.window;
    f.batch(vec![WindowEvent::CursorMoved(CursorMoved {
        window: w,
        position: Vec2::new(point.x, point.y),
        delta: None,
    })]);
    f.batch(vec![button(w, true)]);
    assert!(
        !f.app
            .world()
            .resource::<FrontendScenePointerInput>()
            .has_scene_press()
    );
    assert!(
        f.app
            .world()
            .resource::<FrontendUiLayout>()
            .pointer_captured
    );
    f.batch(vec![button(w, false)]);
    assert_eq!(counts(&f), (0, 0));
}
