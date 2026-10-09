//! Real shared commands, native window batches and the production mesh/canvas plugins.
use crate::*;
use bevy::asset::RenderAssetUsages;
use bevy::camera::{ComputedCameraValues, RenderTargetInfo, visibility::VisibilityPlugin};
use bevy::input::{
    ButtonState, InputPlugin,
    keyboard::{Key, KeyboardInput},
    mouse::MouseButtonInput,
};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent};
use pentimento_ipc::{BevyToUi, PaintCommand, PaintTarget, ProjectCommand, UiToBevy};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static OWNED: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    app: App,
    window: Entity,
    a: Entity,
    b: Entity,
    canvas: Entity,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::var_os("PENTIMENTO_DIRECTUV_DRIVER_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "pentimento-directuv-ui-{}-{}",
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
        .init_resource::<crate::project::ProjectState>()
        .init_resource::<EditModeState>()
        .add_message::<WindowEvent>()
        .add_message::<CursorMoved>()
        .add_plugins((
            PaintModePlugin,
            PaintingSystemPlugin,
            MeshPaintModePlugin,
            MeshPaintingSystemPlugin,
        ))
        .add_systems(PostUpdate, crate::brush_ui::sync_brush_ui_state);
        #[cfg(feature = "sculpting")]
        app.init_resource::<SculptState>()
            .init_resource::<crate::sculpt_mode::SculptingData>();
        crate::brush_presets::owned_test_catalog(app.world_mut(), path.join("brushes.json"));
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
        let a = mesh(&mut app, 12, 0., true);
        let b = mesh(&mut app, 44, 3., false);
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
                CanvasPlane::new(7, 64, 64, 2., 2.),
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
        }
    }
    fn command(&mut self, c: PaintCommand) {
        assert!(dispatch_brush_ui_command(
            self.app.world_mut(),
            &UiToBevy::PaintCommand(c)
        ));
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
        dispatch_brush_ui_command(self.app.world_mut(), &UiToBevy::ProjectCommand(c));
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
        dispatch_brush_ui_command(self.app.world_mut(), &UiToBevy::RequestBrushState);
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
        serde_json::json!({"messages":messages,"image":self.image(),"pixel_bits":pixels,"project_path":self.path.join("painted.pentimento.json"),"undo":r.undo_count(12),"redo":r.redo_count(12)})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
fn mesh(app: &mut App, id: u32, x: f32, textured: bool) -> Entity {
    let geometry = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Rectangle::new(2., 2.));
    let texture = textured.then(|| {
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new(
                Extent3d {
                    width: 64,
                    height: 64,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                (0..4096)
                    .flat_map(|i| [(i % 64 * 3) as u8, 128, 200, 175])
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
                    resolution: (64, 64),
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
    crate::mesh_uv_history::same_pixels(a, b)
}

#[test]
fn mode_brush_catalog_and_window_gesture_use_one_backend_with_exact_units() {
    let mut f = Fixture::new();
    let source = f.source();
    let blank = f.raw();
    let canvas_brush = crate::direct_uv_tool::brush(f.app.world())
        .unwrap()
        .brush_preset;
    f.direct();
    assert_eq!(
        *f.app.world().get::<Visibility>(f.canvas).unwrap(),
        Visibility::Hidden
    );
    assert!(!f.app.world().resource::<ActiveCanvasPlane>().camera_locked);
    f.command(PaintCommand::SelectBrushPreset { preset_id: 2 });
    f.command(PaintCommand::SetBrushSize { size: 8. });
    f.command(PaintCommand::SetBrushOpacity { opacity: 0.4 });
    f.command(PaintCommand::SetBrushSpacing { spacing: 0.2 });
    f.command(PaintCommand::SetBrushColor {
        color: [0.9, 0.1, 0.6, 0.75],
    });
    f.command(PaintCommand::SaveBrushPreset {
        name: "Direct tip".into(),
    });
    f.command(PaintCommand::SetBrushSize { size: 16. });
    f.command(PaintCommand::SelectSavedBrushPreset { preset_id: 1 });
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .brush_preset
            .max_size,
        8.
    );
    assert_eq!(
        f.app.world().resource::<PaintingResource>().brush_preset,
        canvas_brush
    );
    f.gesture();
    assert!(!same(&blank, &f.raw()));
    assert_eq!(f.source(), source);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        1
    );
    let raw = f.raw();
    let image = f.image();
    f.command(PaintCommand::Undo);
    f.settle();
    assert!(same(&blank, &f.raw()));
    f.command(PaintCommand::Redo);
    f.settle();
    assert!(same(&raw, &f.raw()));
    assert_eq!(f.image(), image);
    f.command(PaintCommand::SetTarget {
        target: PaintTarget::Canvas,
    });
    assert_eq!(
        crate::direct_uv_tool::brush(f.app.world())
            .unwrap()
            .brush_preset,
        canvas_brush
    );
    assert_eq!(
        *f.app.world().get::<Visibility>(f.canvas).unwrap(),
        Visibility::Visible
    );
    assert!(matches!(
        crate::brush_presets::message(f.app.world()),
        Some(BevyToUi::SavedBrushPresetsChanged {
            selected_paint: None,
            ..
        })
    ));
    let direct = f.raw();
    f.gesture();
    assert_eq!(f.raw(), direct);
    assert_ne!(f.source(), source);
}

#[test]
fn layered_strokes_history_cancel_and_owned_open_keep_exact_pixels_appearance_and_brush() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 10. });
    f.command(PaintCommand::SetBrushOpacity { opacity: 0.4 });
    f.command(PaintCommand::SetBrushColor {
        color: [0.9, 0.1, 0.5, 0.75],
    });
    f.gesture();
    let first = f.raw();
    let first_image = f.image();
    f.command(PaintCommand::SetBrushColor {
        color: [0.1, 0.8, 0.3, 0.6],
    });
    f.gesture();
    let second = f.raw();
    let second_image = f.image();
    assert!(!same(&first, &second));
    f.command(PaintCommand::Undo);
    f.settle();
    assert!(same(&first, &f.raw()));
    assert_eq!(first_image, f.image());
    f.command(PaintCommand::Redo);
    f.settle();
    assert!(same(&second, &f.raw()));
    assert_eq!(second_image, f.image());
    let color = f.app.world().resource::<MeshPaintingResource>().brush_color;
    let path = f.path.join("layered.pentimento.json");
    assert!(f.save(&path));
    f.batch(vec![
        movement(f.window, 500.),
        button(f.window, true),
        movement(f.window, 600.),
    ]);
    assert!(!f.save(&path));
    f.command(PaintCommand::CancelStroke);
    f.settle();
    assert!(same(&second, &f.raw()));
    assert_eq!(f.image(), second_image);
    f.batch(vec![button(f.window, false)]);
    f.settle();
    assert!(f.open(&path));
    f.settle();
    assert!(same(&second, &f.raw()));
    assert_eq!(f.image(), second_image);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        0
    );
    // Real Shift+Tab re-enters the existing paint mode after fresh Open.
    f.batch(vec![
        key(f.window, KeyCode::ShiftLeft, true),
        key(f.window, KeyCode::Tab, true),
    ]);
    f.batch(vec![
        key(f.window, KeyCode::Tab, false),
        key(f.window, KeyCode::ShiftLeft, false),
    ]);
    f.direct();
    assert_eq!(
        f.app.world().resource::<MeshPaintingResource>().brush_color,
        color
    );
    f.gesture();
    let after = f.raw();
    assert!(!same(&second, &after));
    f.command(PaintCommand::Undo);
    f.settle();
    assert!(same(&second, &f.raw()));
    std::fs::write(&path, b"external owner").unwrap();
    assert!(!f.save(&path));
    assert_eq!(std::fs::read(&path).unwrap(), b"external owner");
    assert!(f.save(&f.path.join("recover.pentimento.json")));
}

#[test]
fn fresh_stationary_press_escape_and_cancel_button_restore_first_original_binding() {
    let mut f = Fixture::new();
    let original = appearance(f.app.world(), f.a);
    let raw = f.raw();
    let image = f.image();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    f.app
        .world_mut()
        .get_mut::<Window>(f.window)
        .unwrap()
        .set_cursor_position(Some(Vec2::splat(500.)));
    f.batch(vec![button(f.window, true)]);
    assert!(!same(&raw, &f.raw()));
    f.batch(vec![
        key(f.window, KeyCode::Escape, true),
        movement(f.window, 650.),
    ]);
    assert!(same(&raw, &f.raw()));
    assert_eq!(appearance(f.app.world(), f.a), original);
    assert_eq!(f.image(), image);
    f.batch(vec![
        key(f.window, KeyCode::Escape, false),
        button(f.window, false),
    ]);
    f.batch(vec![movement(f.window, 500.), button(f.window, true)]);
    f.command(PaintCommand::CancelStroke);
    f.settle();
    assert!(same(&raw, &f.raw()));
    assert_eq!(appearance(f.app.world(), f.a), original);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        0
    );
}

#[test]
fn complete_pointer_then_shortcut_in_one_batch_undoes_that_stroke_and_held_history_refuses() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    let blank = f.raw();
    f.batch(vec![
        movement(f.window, 350.),
        button(f.window, true),
        movement(f.window, 650.),
        button(f.window, false),
        key(f.window, KeyCode::ControlLeft, true),
        key(f.window, KeyCode::KeyZ, true),
        key(f.window, KeyCode::KeyZ, false),
        key(f.window, KeyCode::ControlLeft, false),
    ]);
    assert!(same(&blank, &f.raw()));
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .redo_count(12),
        1
    );
    f.command(PaintCommand::Redo);
    f.settle();
    let painted = f.raw();
    f.batch(vec![
        movement(f.window, 350.),
        button(f.window, true),
        movement(f.window, 650.),
        key(f.window, KeyCode::ControlLeft, true),
        key(f.window, KeyCode::KeyZ, true),
        key(f.window, KeyCode::KeyZ, false),
        key(f.window, KeyCode::ControlLeft, false),
    ]);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        1
    );
    f.command(PaintCommand::SetTarget {
        target: PaintTarget::Canvas,
    });
    f.command(PaintCommand::SelectBrushPreset { preset_id: 1 });
    assert_eq!(
        f.app.world().resource::<PaintMode>().target,
        PaintTarget::DirectUv
    );
    f.command(PaintCommand::CancelStroke);
    f.settle();
    assert!(same(&painted, &f.raw()));
}

#[test]
fn ui_arbitration_focus_and_visibility_preserve_valid_prefix_without_inactive_replay() {
    let mut f = Fixture::new();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 8. });
    f.app
        .world_mut()
        .insert_resource(FrontendScenePointerInput::default());
    f.app
        .world_mut()
        .resource_mut::<FrontendScenePointerInput>()
        .publish(
            f.window,
            vec![
                movement(f.window, 350.),
                button(f.window, true),
                movement(f.window, 650.),
                button(f.window, false),
            ],
        );
    f.app
        .world_mut()
        .resource_mut::<FrontendInputBlockState>()
        .block_pointer = true;
    f.batch(vec![movement(f.window, 800.), button(f.window, true)]);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        1
    );
    let prefix = f.raw();
    f.app
        .world_mut()
        .resource_mut::<FrontendScenePointerInput>()
        .clear();
    f.batch(vec![movement(f.window, 800.), button(f.window, false)]);
    assert!(same(&prefix, &f.raw()));
    f.app
        .world_mut()
        .resource_mut::<FrontendInputBlockState>()
        .block_pointer = false;
    f.app
        .world_mut()
        .get_mut::<Visibility>(f.a)
        .map(|mut v| *v = Visibility::Hidden);
    f.settle();
    f.gesture();
    assert!(same(&prefix, &f.raw()));
    f.app
        .world_mut()
        .get_mut::<Visibility>(f.a)
        .map(|mut v| *v = Visibility::Visible);
    f.settle();
    f.batch(vec![
        movement(f.window, 350.),
        button(f.window, true),
        movement(f.window, 650.),
        WindowEvent::WindowFocused(bevy::window::WindowFocused {
            window: f.window,
            focused: false,
        }),
    ]);
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        2
    );
    let prefix = f.raw();
    f.batch(vec![
        WindowEvent::WindowFocused(bevy::window::WindowFocused {
            window: f.window,
            focused: true,
        }),
        button(f.window, false),
    ]);
    assert!(same(&prefix, &f.raw()));
}

#[test]
fn unsupported_foreground_surface_and_canvas_only_controls_cannot_mutate_direct_work() {
    let mut f = Fixture::new();
    f.direct();
    let raw = f.raw();
    let source = f.source();
    for command in [
        PaintCommand::AddLayer {
            name: "wrong backend".into(),
        },
        PaintCommand::ProjectToScene,
        PaintCommand::SetColorSampling { enabled: true },
    ] {
        f.command(command);
    }
    assert_eq!(f.source(), source);
    assert!(same(&raw, &f.raw()));
    let occluder = mesh(&mut f.app, 99, 0., false);
    f.app.world_mut().entity_mut(occluder).insert((
        Transform::from_xyz(0., 0., 0.5),
        PaintableMesh {
            mesh_id: 99,
            storage_mode: painting::MeshStorageMode::Ptex { face_resolution: 8 },
        },
    ));
    f.settle();
    f.gesture();
    assert!(same(&raw, &f.raw()));
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        0
    );
    assert!(
        f.app
            .world()
            .resource::<PaintMode>()
            .target_notice
            .as_ref()
            .unwrap()
            .contains("PTex")
    );
}

#[test]
#[ignore = "interactive Svelte-to-real-Bevy CPU texture qualification driver"]
fn browser_driver() {
    use std::io::{BufRead, Write};
    let mut f = Fixture::new();
    println!("PENTIMENTO_DRIVER {}", f.state());
    std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        if line.is_empty() {
            continue;
        }
        let input: serde_json::Value = serde_json::from_str(&line).unwrap();
        if input["stop"].as_bool() == Some(true) {
            break;
        }
        if let Some(ui) = input.get("ui") {
            let command: UiToBevy = serde_json::from_value(ui.clone()).unwrap();
            dispatch_brush_ui_command(f.app.world_mut(), &command);
            f.app.update();
            if f.app.world().get_entity(f.a).is_err() {
                f.a = named(f.app.world_mut(), "UV12");
                f.b = named(f.app.world_mut(), "UV44");
                f.canvas = named(f.app.world_mut(), "Source");
            }
        }
        if let Some(pointer) = input.get("pointer") {
            let x = pointer["x"].as_f64().unwrap_or(500.) as f32;
            let event = match pointer["kind"].as_str().unwrap() {
                "move" => movement(f.window, x),
                "down" => button(f.window, true),
                "up" => button(f.window, false),
                _ => panic!("unknown pointer"),
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
                _ => panic!("unsupported key"),
            };
            f.batch(vec![key(f.window, code, action["down"].as_bool().unwrap())]);
        }
        f.settle();
        println!("PENTIMENTO_DRIVER {}", f.state());
        std::io::stdout().flush().unwrap();
    }
}

#[test]
fn disconnected_uv_faces_do_not_interpolate_across_the_atlas_and_undo_is_exact() {
    use bevy::mesh::{Indices, PrimitiveTopology};
    let mut f = Fixture::new();
    let handle = f.app.world().get::<Mesh3d>(f.a).unwrap().0.clone();
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-1., -1., 0.],
            [1., -1., 0.],
            [1., 1., 0.],
            [-1., -1., 0.],
            [1., 1., 0.],
            [-1., 1., 0.],
        ],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![
            [0.05, 0.1],
            [0.2, 0.1],
            [0.2, 0.9],
            [0.8, 0.1],
            [0.95, 0.9],
            [0.8, 0.9],
        ],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 3, 4, 5]));
    assert!(!crate::mesh_painting_system::uv_faces_continuous(
        &mesh, 0, 1
    ));
    *f.app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&handle)
        .unwrap() = mesh;
    f.settle();
    f.direct();
    f.command(PaintCommand::SetBrushSize { size: 4. });
    let before = f.raw();
    f.gesture();
    f.settle();
    let after = f.raw();
    assert!(!same(&before, &after));
    // Both faces receive local dabs; the atlas middle is not a connecting line.
    for y in 0..64 {
        for x in 22..42 {
            let i = y * 64 + x;
            assert_eq!(after[i], before[i]);
        }
    }
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        1
    );
    f.command(PaintCommand::Undo);
    f.settle();
    assert!(same(&before, &f.raw()));
    let continuous = Mesh::from(Rectangle::new(2., 2.));
    assert!(crate::mesh_painting_system::uv_faces_continuous(
        &continuous,
        0,
        1
    ));
}

#[test]
fn source_owner_is_restored_on_exit_and_canvas_replacement_is_refused_in_direct() {
    use bevy::ecs::system::RunSystemOnce;
    let mut f = Fixture::new();
    f.direct();
    let other = f
        .app
        .world_mut()
        .spawn((CanvasPlane::new(8, 64, 64, 2., 2.), Visibility::Visible))
        .id();
    f.app.world_mut().init_resource::<CanvasPlaneIdGenerator>();
    f.app.add_message::<CanvasPlaneEvent>();
    for event in [
        CanvasPlaneEvent::Select(other),
        CanvasPlaneEvent::CreateInFrontOfCamera {
            width: 64,
            height: 64,
        },
        CanvasPlaneEvent::Deselect,
    ] {
        f.app.world_mut().write_message(event);
        f.app
            .world_mut()
            .run_system_once(crate::canvas_plane::handle_canvas_plane_events)
            .unwrap();
        assert_eq!(
            f.app.world().resource::<ActiveCanvasPlane>().entity,
            Some(f.canvas)
        );
        assert_eq!(
            f.app.world().get::<Visibility>(f.canvas),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            f.app.world().get::<Visibility>(other),
            Some(&Visibility::Visible)
        );
    }
    f.batch(vec![
        key(f.window, KeyCode::ShiftLeft, true),
        key(f.window, KeyCode::Tab, true),
    ]);
    assert!(!f.app.world().resource::<PaintMode>().active);
    assert_eq!(
        f.app.world().get::<Visibility>(f.canvas),
        Some(&Visibility::Visible)
    );
    assert!(f.app.world().resource::<ActiveCanvasPlane>().camera_locked);
}

#[test]
fn pending_projection_and_unreported_focus_loss_are_settled_without_cross_backend_work() {
    let mut f = Fixture::new();
    f.app.add_message::<ProjectionEvent>();
    f.app
        .world_mut()
        .write_message(ProjectionEvent::ProjectToScene);
    f.command(PaintCommand::SetTarget {
        target: PaintTarget::DirectUv,
    });
    assert_eq!(
        f.app.world().resource::<PaintMode>().target,
        PaintTarget::Canvas
    );
    f.app
        .world_mut()
        .resource_mut::<bevy::ecs::message::Messages<ProjectionEvent>>()
        .clear();
    f.direct();
    f.batch(vec![
        movement(f.window, 450.),
        button(f.window, true),
        movement(f.window, 550.),
    ]);
    let prefix = f.raw();
    f.app
        .world_mut()
        .get_mut::<Window>(f.window)
        .unwrap()
        .focused = false;
    f.settle();
    assert!(
        f.app
            .world()
            .resource::<MeshPaintState>()
            .current_stroke
            .is_none()
    );
    assert_eq!(
        f.app
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        1
    );
    assert!(same(&prefix, &f.raw()));
}
