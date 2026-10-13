use super::*;
use crate::projection_painting::tests::{output, set_canvas, test_app};
use std::sync::atomic::{AtomicU64, Ordering};
static OWNED: AtomicU64 = AtomicU64::new(0);
struct OwnedDirectory(PathBuf);
impl OwnedDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pentimento-project-v1-{}-{}",
            std::process::id(),
            OWNED.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}.pentimento.json"))
    }
}
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn editor() -> (App, Entity) {
    let (mut app, target, _) = test_app();
    app.init_resource::<ProjectState>()
        .init_resource::<OutboundUiMessages>()
        .init_resource::<CanvasPlaneIdGenerator>()
        .init_resource::<crate::StrokeIdGenerator>();
    #[cfg(feature = "sculpting")]
    app.init_resource::<crate::SculptState>()
        .init_resource::<crate::sculpt_mode::SculptingData>();
    let canvas = app
        .world()
        .resource::<crate::ActiveCanvasPlane>()
        .entity
        .unwrap();
    let mh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Rectangle::new(2., 2.));
    let mat = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            double_sided: true,
            ..default()
        });
    app.world_mut().entity_mut(canvas).insert((
        Mesh3d(mh),
        MeshMaterial3d(mat),
        Visibility::Visible,
    ));
    app.world_mut()
        .entity_mut(target)
        .insert(Name::new("Receiver"));
    app.world_mut()
        .resource_mut::<crate::ProjectionMode>()
        .live_projection = false;
    (app, target)
}
fn command(app: &mut App, c: ProjectCommand) -> bool {
    assert!(crate::dispatch_brush_ui_command(
        app.world_mut(),
        &pentimento_ipc::UiToBevy::ProjectCommand(c)
    ));
    app.world()
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
fn save(app: &mut App, path: &Path) -> bool {
    command(
        app,
        ProjectCommand::Save {
            path: path.to_string_lossy().into_owned(),
        },
    )
}
fn open(app: &mut App, path: &Path) -> bool {
    command(
        app,
        ProjectCommand::Open {
            path: path.to_string_lossy().into_owned(),
        },
    )
}
fn document(app: &mut App) -> ProjectDocument {
    capture(app.world_mut()).unwrap().0
}
fn receiver(app: &mut App) -> Entity {
    app.world_mut()
        .query::<(Entity, &Name)>()
        .iter(app.world())
        .find(|(_, n)| n.as_str() == "Receiver")
        .unwrap()
        .0
}
fn counts(app: &App) -> (usize, usize, usize) {
    (
        app.world().resource::<Assets<Mesh>>().len(),
        app.world().resource::<Assets<Image>>().len(),
        app.world().resource::<Assets<StandardMaterial>>().len(),
    )
}
fn pixels(app: &App) -> Vec<[f32; 4]> {
    app.world()
        .resource::<PaintingResource>()
        .get_pipeline(0)
        .unwrap()
        .layers
        .active_layer()
        .unwrap()
        .surface
        .surface()
        .pixels()
        .to_vec()
}

#[test]
fn unsupported_version_is_clear() {
    let error =
        parse(br#"{"format":"pentimento-project","version":500,"future":true}"#).unwrap_err();
    assert!(error.contains("Unsupported Pentimento project version 500"));
}

#[test]
fn owned_file_roundtrip_keeps_layer_identity_bits_and_real_first_edit_history() {
    let dir = OwnedDirectory::new();
    let path = dir.file("layers");
    let (mut app, _) = editor();
    {
        let mut p = app.world_mut().resource_mut::<PaintingResource>();
        let pipeline = p.get_pipeline_mut(0).unwrap();
        let removed = pipeline.layers.add_layer("deleted".into());
        assert!(pipeline.layers.remove_layer(removed));
        let top = pipeline.layers.add_layer("exact float pixels".into());
        pipeline.layers.reorder(top, 0);
        pipeline.layers.set_opacity(top, 0.375);
        pipeline
            .layers
            .active_layer_mut()
            .unwrap()
            .surface
            .surface_mut()
            .pixels_mut()[0] = [-0.0, f32::from_bits(1), f32::from_bits(0x3eaaaaab), 0.5];
        pipeline.begin_stroke(0, 91, 0);
        pipeline.stroke_to(1., 1., 1.);
        pipeline.end_stroke();
        assert!(pipeline.can_undo());
        pipeline
            .layers
            .active_layer_mut()
            .unwrap()
            .surface
            .surface_mut()
            .pixels_mut()[0] = [-0.0, f32::from_bits(1), f32::from_bits(0x3eaaaaab), 0.5];
        pipeline.layers.composite();
    }
    let before = document(&mut app);
    let ids = before
        .objects
        .iter()
        .find_map(|o| o.canvas.as_ref())
        .unwrap()
        .layers
        .clone();
    assert_eq!(ids.next_id, 3);
    assert_eq!(ids.active_layer_id, 2);
    assert!(save(&mut app, &path));
    let bytes = read_bounded(&path).unwrap();
    let (mut reopened, _) = editor();
    let camera = reopened
        .world_mut()
        .spawn((crate::MainCamera, Transform::default()))
        .id();
    assert!(open(&mut reopened, &path));
    assert!(reopened.world().get_entity(camera).is_ok());
    let after = document(&mut reopened);
    assert_eq!(serialize(&before).unwrap(), serialize(&after).unwrap());
    let baseline = pixels(&reopened);
    let mut p = reopened.world_mut().resource_mut::<PaintingResource>();
    let pipeline = p.get_pipeline_mut(0).unwrap();
    assert!(!pipeline.can_undo() && !pipeline.can_redo());
    assert!(pipeline.log().total_packet_count() == 0);
    pipeline.set_color([0., 1., 0., 1.]);
    pipeline.begin_stroke(0, 92, 0);
    pipeline.stroke_to(0., 0., 1.);
    pipeline.end_stroke();
    let edited = pipeline
        .layers
        .active_layer()
        .unwrap()
        .surface
        .surface()
        .pixels()
        .to_vec();
    assert_ne!(baseline, edited);
    assert!(pipeline.undo());
    assert_eq!(
        baseline,
        pipeline
            .layers
            .active_layer()
            .unwrap()
            .surface
            .surface()
            .pixels()
    );
    assert!(pipeline.redo());
    assert_eq!(
        edited,
        pipeline
            .layers
            .active_layer()
            .unwrap()
            .surface
            .surface()
            .pixels()
    );
    assert_eq!(
        parse(&bytes)
            .unwrap()
            .objects
            .iter()
            .find_map(|o| o.canvas.as_ref())
            .unwrap()
            .layers
            .next_id,
        3
    );
}

#[test]
fn raw_attributes_index_width_order_and_missing_assets_are_explicit() {
    use bevy::mesh::{Indices, VertexAttributeValues};
    let (mut app, target) = editor();
    let handle = app.world().get::<Mesh3d>(target).unwrap().0.clone();
    {
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let mut mesh = meshes.get_mut(&handle).unwrap();
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1., 0., 0., -1.]; 4]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.1, 0.2, 0.3, 0.4]; 4]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.25, 0.75]; 4]);
        mesh.insert_indices(Indices::U16(vec![2, 0, 1, 3, 0, 2]));
    }
    let raw = MeshDocument::capture(app.world().resource::<Assets<Mesh>>().get(&handle).unwrap())
        .unwrap();
    let bytes = serde_json::to_vec(&raw).unwrap();
    let restored: MeshDocument = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    let mesh = restored.restore();
    assert!(raw.matches(&mesh));
    assert!(matches!(mesh.indices(), Some(Indices::U16(_))));
    assert!(matches!(
        mesh.attribute(Mesh::ATTRIBUTE_UV_1),
        Some(VertexAttributeValues::Float32x2(_))
    ));
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .remove(handle.id());
    assert!(
        capture(app.world_mut())
            .unwrap_err()
            .contains("Missing project mesh asset")
    );
}

#[test]
fn rejected_parse_budget_and_reference_loads_leave_document_assets_path_and_history() {
    let dir = OwnedDirectory::new();
    let good = dir.file("good");
    let bad = dir.file("bad");
    let (mut app, _) = editor();
    assert!(save(&mut app, &good));
    let before = serialize(&document(&mut app)).unwrap();
    let assets = counts(&app);
    let generation = project_generation(app.world());
    for mutate in [0, 1, 2, 3, 4] {
        let mut value: serde_json::Value = serde_json::from_slice(&before).unwrap();
        match mutate {
            0 => value["version"] = 99.into(),
            1 => value["objects"][0]["mesh"]["positions"] = serde_json::json!([]),
            2 => value["next_plane_id"] = u32::MAX.into(),
            3 => {
                let i = value["objects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|o| !o["canvas"].is_null())
                    .unwrap();
                value["objects"][i]["canvas"]["layers"]["width"] = u32::MAX.into();
            }
            _ => value["unknown_required"] = "extension".into(),
        };
        std::fs::write(&bad, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(!open(&mut app, &bad));
        assert_eq!(counts(&app), assets);
        assert_eq!(project_generation(app.world()), generation);
        assert_eq!(
            app.world().resource::<ProjectState>().path.as_ref(),
            Some(&good)
        );
        assert_eq!(serialize(&document(&mut app)).unwrap(), before);
    }
}

#[test]
fn active_pipeline_and_scene_ownership_refuse_save_open_without_abandoning_stroke() {
    let dir = OwnedDirectory::new();
    let path = dir.file("active");
    let (mut app, _) = editor();
    assert!(save(&mut app, &path));
    let saved = read_bounded(&path).unwrap();
    app.world_mut()
        .resource_mut::<PaintingResource>()
        .get_pipeline_mut(0)
        .unwrap()
        .begin_stroke(0, 55, 0);
    assert!(!save(&mut app, &path));
    assert!(!open(&mut app, &path));
    assert_eq!(read_bounded(&path).unwrap(), saved);
    assert!(
        app.world()
            .resource::<PaintingResource>()
            .get_pipeline(0)
            .unwrap()
            .is_stroking()
    );
    app.world_mut()
        .resource_mut::<PaintingResource>()
        .get_pipeline_mut(0)
        .unwrap()
        .cancel_stroke();
    app.world_mut().insert_resource(crate::PaintMode {
        current_stroke: Some(crate::StrokeState {
            stroke_id: 4,
            space_id: 0,
            start_time: 0,
            last_world_pos: None,
            last_time: 0.,
        }),
        ..default()
    });
    assert!(!open(&mut app, &path));
    assert!(
        app.world()
            .resource::<crate::PaintMode>()
            .current_stroke
            .is_some()
    );
}

#[test]
fn failed_or_interrupted_save_keeps_original_and_cleans_temporary_files() {
    let dir = OwnedDirectory::new();
    let path = dir.file("atomic");
    atomic_save(&path, b"old", None).unwrap();
    assert!(
        atomic_save_with(&path, b"new", Some(b"old"), || Err(
            "injected interruption before commit".into()
        ))
        .is_err()
    );
    assert_eq!(read_bounded(&path).unwrap(), b"old");
    assert!(
        !std::fs::read_dir(&dir.0).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|e| e == "tmp"))
    );
    atomic_save(&path, b"new", Some(b"old")).unwrap();
    assert_eq!(read_bounded(&path).unwrap(), b"new");
    assert!(atomic_save(&path, b"overwrite", None).is_err());
    assert!(atomic_save(&dir.file("absent/subdir"), b"new", None).is_err());
    assert_eq!(read_bounded(&path).unwrap(), b"new");
}

#[cfg(unix)]
#[test]
fn owned_atomic_replacement_preserves_private_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = OwnedDirectory::new();
    let path = dir.file("private");
    atomic_save(&path, b"old", None).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    atomic_save(&path, b"new", Some(b"old")).unwrap();
    assert_eq!(read_bounded(&path).unwrap(), b"new");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn external_edit_conflict_blocks_same_file_and_save_as_recovers_without_overwrite() {
    let dir = OwnedDirectory::new();
    let path = dir.file("owned");
    let other = dir.file("recovery");
    let (mut app, _) = editor();
    assert!(save(&mut app, &path));
    std::fs::write(&path, b"external edit").unwrap();
    assert!(!save(&mut app, &path));
    assert!(app.world().resource::<ProjectState>().blocked);
    assert_eq!(read_bounded(&path).unwrap(), b"external edit");
    assert!(save(&mut app, &other));
    assert!(!app.world().resource::<ProjectState>().blocked);
    assert_eq!(read_bounded(&path).unwrap(), b"external edit");
}

#[test]
fn project_io_rejects_nonregular_large_and_symbolic_destinations() {
    let dir = OwnedDirectory::new();
    assert!(
        read_bounded(&dir.0)
            .unwrap_err()
            .contains("not a regular file")
    );
    let path = dir.file("large");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_BYTES + 1).unwrap();
    assert!(read_bounded(&path).unwrap_err().contains("64 MiB"));
    #[cfg(unix)]
    {
        let link = dir.file("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_bounded(&link).is_err());
        assert!(atomic_save(&link, b"no", None).is_err());
    }
}

#[test]
fn projection_layers_original_appearance_accepted_material_edits_and_clear_reopen_exactly() {
    for clear in [false, true] {
        let dir = OwnedDirectory::new();
        let path = dir.file("projected");
        let (mut app, target) = editor();
        app.world_mut()
            .resource_mut::<crate::ProjectionMode>()
            .live_projection = true;
        set_canvas(&mut app, &[[0.2, 0.4, 0.8, 0.5]; 4]);
        app.update();
        let bound = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(target)
            .unwrap()
            .0
            .clone();
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&bound)
            .unwrap()
            .perceptual_roughness = 0.3125;
        if clear {
            app.world_mut()
                .write_message(crate::ProjectionEvent::ClearAllProjections);
            app.update();
            app.world_mut()
                .resource_mut::<Messages<crate::ProjectionEvent>>()
                .clear();
        }
        let expected = output(&app, target);
        assert!(save(&mut app, &path));
        let (mut reopened, _) = editor();
        assert!(open(&mut reopened, &path));
        let target = receiver(&mut reopened);
        assert!(
            !reopened
                .world()
                .resource::<crate::ProjectionMode>()
                .live_projection
        );
        reopened.update();
        assert_eq!(expected, output(&reopened, target));
        reopened.update();
        assert_eq!(
            expected,
            output(&reopened, target),
            "open must not reproject or double-blend"
        );
        if !clear {
            let handle = &reopened
                .world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0;
            assert_eq!(
                reopened
                    .world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(handle)
                    .unwrap()
                    .perceptual_roughness,
                0.3125
            );
        }
    }
}

#[test]
fn live_undo_save_waits_for_receiver_then_reopens_exact_source_and_rendered_pixels() {
    let dir = OwnedDirectory::new();
    let path = dir.file("live-undo");
    let (mut app, target) = editor();
    app.world_mut()
        .resource_mut::<crate::ProjectionMode>()
        .live_projection = true;
    {
        let mut painting = app.world_mut().resource_mut::<PaintingResource>();
        let pipeline = painting.get_pipeline_mut(0).unwrap();
        pipeline.set_color([1., 0., 0., 1.]);
        pipeline.begin_stroke(0, 1, 0);
        pipeline.stroke_to(0.5, 0.5, 1.);
        pipeline.end_stroke();
    }
    app.update();
    let painted = output(&app, target);
    assert!(save(&mut app, &path));
    let previous = read_bounded(&path).unwrap();
    assert!(crate::dispatch_brush_ui_command(
        app.world_mut(),
        &pentimento_ipc::UiToBevy::PaintCommand(pentimento_ipc::PaintCommand::Undo)
    ));
    assert!(
        !save(&mut app, &path),
        "Save cannot freeze a receiver from before Undo"
    );
    assert_eq!(read_bounded(&path).unwrap(), previous);
    assert_eq!(output(&app, target), painted);
    app.update();
    let expected = output(&app, target);
    assert_ne!(expected, painted);
    let source = app
        .world()
        .resource::<PaintingResource>()
        .get_pipeline(0)
        .unwrap()
        .layers
        .document();
    assert!(save(&mut app, &path));
    let (mut reopened, _) = editor();
    assert!(open(&mut reopened, &path));
    reopened.update();
    let target = receiver(&mut reopened);
    assert_eq!(expected, output(&reopened, target));
    assert_eq!(
        serde_json::to_vec(&source).unwrap(),
        serde_json::to_vec(
            &reopened
                .world()
                .resource::<PaintingResource>()
                .get_pipeline(0)
                .unwrap()
                .layers
                .document()
        )
        .unwrap()
    );
    assert!(
        !reopened
            .world()
            .resource::<crate::ProjectionMode>()
            .live_projection
    );
}

#[test]
fn live_mapping_changes_refuse_save_until_projection_matches_current_authoring_inputs() {
    for change in [
        "transform",
        "visibility",
        "camera",
        "cull",
        "delete",
        "vertices",
        "uv",
    ] {
        let dir = OwnedDirectory::new();
        let path = dir.file(change);
        let (mut app, target) = editor();
        let camera = app
            .world_mut()
            .query_filtered::<Entity, With<crate::MainCamera>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .entity_mut(camera)
            .insert(crate::OrbitCamera {
                distance: 2.,
                yaw: 0.,
                pitch: 0.,
                ..default()
            });
        app.world_mut()
            .resource_mut::<crate::ProjectionMode>()
            .live_projection = true;
        set_canvas(&mut app, &[[1., 0., 0., 1.]; 4]);
        app.update();
        assert!(save(&mut app, &path));
        let previous = read_bounded(&path).unwrap();
        match change {
            "transform" => {
                app.world_mut()
                    .get_mut::<Transform>(target)
                    .unwrap()
                    .translation
                    .x = 10.
            }
            "visibility" => {
                app.world_mut()
                    .entity_mut(target)
                    .insert(Visibility::Hidden);
            }
            "camera" => {
                app.world_mut()
                    .get_mut::<crate::OrbitCamera>(camera)
                    .unwrap()
                    .yaw = 0.8
            }
            "cull" => {
                let handle = app
                    .world()
                    .get::<MeshMaterial3d<StandardMaterial>>(target)
                    .unwrap()
                    .0
                    .clone();
                app.world_mut()
                    .resource_mut::<Assets<StandardMaterial>>()
                    .get_mut(&handle)
                    .unwrap()
                    .cull_mode = None;
            }
            "delete" => {
                app.world_mut().despawn(target);
            }
            "vertices" | "uv" => {
                let handle = app.world().get::<Mesh3d>(target).unwrap().0.clone();
                let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
                let mut mesh = meshes.get_mut(&handle).unwrap();
                if change == "vertices" {
                    let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
                        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("positions");
                    };
                    positions[0][0] += 0.5;
                } else {
                    let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) =
                        mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
                    else {
                        panic!("UV0");
                    };
                    uv[0][0] = 0.25;
                }
            }
            _ => unreachable!(),
        }
        assert!(
            !save(&mut app, &path),
            "{change} cannot freeze an older live mapping"
        );
        assert_eq!(read_bounded(&path).unwrap(), previous);
        // This headless fixture installs TransformPlugin; mirror the production
        // camera/visibility systems before the actual projection PostUpdate runs.
        if change == "camera" {
            let orbit = app.world().get::<crate::OrbitCamera>(camera).unwrap();
            let transform = Transform::from_translation(orbit.calculate_position())
                .looking_at(orbit.target, Vec3::Y);
            *app.world_mut().get_mut::<Transform>(camera).unwrap() = transform;
        }
        if change == "visibility" {
            app.world_mut()
                .entity_mut(target)
                .insert(InheritedVisibility::HIDDEN);
        }
        for _ in 0..6 {
            app.update();
        }
        assert!(
            save(&mut app, &path),
            "{change} must save once settled: {:?}",
            app.world().resource::<ProjectState>().notice
        );
        let expected = document(&mut app);
        let expected_pixels = (change != "delete").then(|| output(&app, target));
        let (mut reopened, _) = editor();
        assert!(open(&mut reopened, &path));
        reopened.update();
        let actual = document(&mut reopened);
        assert_eq!(expected.objects.len(), actual.objects.len());
        if let Some(pixels) = expected_pixels {
            let receiver = receiver(&mut reopened);
            assert_eq!(
                pixels,
                output(&reopened, receiver),
                "{change} rendered pixels"
            );
        }
    }
}

#[test]
fn multiple_canvas_projection_layers_and_inactive_source_identities_roundtrip() {
    let dir = OwnedDirectory::new();
    let path = dir.file("multiple");
    let (mut app, target) = editor();
    app.world_mut()
        .resource_mut::<crate::ProjectionMode>()
        .live_projection = true;
    set_canvas(&mut app, &[[1., 0., 0., 0.5]; 4]);
    app.update();
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Rectangle::new(2., 2.));
    let mat = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        });
    let second = app
        .world_mut()
        .spawn((
            CanvasPlane::new(1, 2, 2, 2., 2.),
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_xyz(0., 0., 1.),
            Visibility::Hidden,
        ))
        .id();
    app.world_mut()
        .resource_mut::<PaintingResource>()
        .get_or_create_pipeline(1, 2, 2)
        .layers
        .active_layer_mut()
        .unwrap()
        .surface
        .surface_mut()
        .clear([0., 0., 1., 0.5]);
    app.world_mut()
        .resource_mut::<crate::ActiveCanvasPlane>()
        .entity = Some(second);
    app.update();
    assert_eq!(
        app.world()
            .resource::<ProjectionTargets>()
            .document_layers(target)
            .len(),
        2
    );
    let expected = output(&app, target);
    assert!(save(&mut app, &path));
    let (mut fresh, _) = editor();
    assert!(open(&mut fresh, &path));
    let target = receiver(&mut fresh);
    fresh.update();
    assert_eq!(output(&fresh, target), expected);
    assert_eq!(
        fresh
            .world()
            .resource::<ProjectionTargets>()
            .document_layers(target)
            .len(),
        2
    );
    assert!(
        fresh
            .world()
            .resource::<PaintingResource>()
            .get_pipeline(0)
            .is_some()
    );
    assert!(
        fresh
            .world()
            .resource::<PaintingResource>()
            .get_pipeline(1)
            .is_some()
    );
    let active = fresh
        .world()
        .resource::<crate::ActiveCanvasPlane>()
        .entity
        .unwrap();
    assert_eq!(
        fresh.world().get::<CanvasPlane>(active).unwrap().plane_id,
        1
    );
    assert_eq!(
        *fresh.world().get::<Visibility>(active).unwrap(),
        Visibility::Hidden
    );
}

#[test]
fn open_discards_pending_input_and_publishes_new_document_state_even_when_brush_unchanged() {
    use bevy::input::mouse::{MouseMotion, MouseWheel};
    let dir = OwnedDirectory::new();
    let path = dir.file("input");
    let (mut app, _) = editor();
    app.add_plugins(bevy::input::InputPlugin);
    app.add_message::<crate::CanvasPlaneEvent>()
        .add_message::<crate::PaintEvent>()
        .add_plugins(crate::AddObjectPlugin);
    assert!(save(&mut app, &path));
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Tab);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ShiftLeft);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut().write_message(MouseMotion {
        delta: Vec2::splat(500.),
    });
    app.world_mut()
        .write_message(crate::CanvasPlaneEvent::CreateInFrontOfCamera {
            width: 2,
            height: 2,
        });
    app.world_mut()
        .resource_mut::<OutboundUiMessages>()
        .send(BevyToUi::EditModeChanged {
            mode: pentimento_ipc::EditMode::Sculpt,
        });
    assert!(open(&mut app, &path));
    assert!(app.world().resource::<Messages<MouseMotion>>().is_empty());
    assert!(app.world().resource::<Messages<MouseWheel>>().is_empty());
    assert!(
        app.world()
            .resource::<Messages<crate::CanvasPlaneEvent>>()
            .is_empty()
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::Tab)
    );
    assert!(
        !app.world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
    let messages = &app.world().resource::<OutboundUiMessages>().messages;
    assert!(!messages.iter().any(|m| matches!(
        m,
        BevyToUi::EditModeChanged {
            mode: pentimento_ipc::EditMode::Sculpt
        }
    )));
    assert!(messages.iter().any(|m| matches!(
        m,
        BevyToUi::EditModeChanged {
            mode: pentimento_ipc::EditMode::None
        }
    )));
    assert!(messages.iter().any(|m| matches!(
        m,
        BevyToUi::ProjectionModeChanged {
            live_projection: false
        }
    )));
    assert!(messages.iter().any(|m| matches!(
        m,
        BevyToUi::PaintBrushStateChanged {
            can_undo: false,
            can_redo: false,
            ..
        }
    )));
    assert!(
        messages
            .iter()
            .any(|m| matches!(m,BevyToUi::LayerStateChanged{layers} if !layers.is_empty()))
    );
}

#[cfg(feature = "selection")]
#[test]
fn opened_primitive_selection_ids_are_reseeded_before_real_add_object() {
    let dir = OwnedDirectory::new();
    let path = dir.file("ids");
    let (mut app, target) = editor();
    app.world_mut()
        .entity_mut(target)
        .insert(crate::Selectable {
            id: "object_82".into(),
        });
    assert!(save(&mut app, &path));
    let (mut reopened, _) = editor();
    reopened.add_plugins(crate::AddObjectPlugin);
    assert!(open(&mut reopened, &path));
    reopened
        .world_mut()
        .write_message(crate::AddObjectEvent(pentimento_ipc::AddObjectRequest {
            primitive_type: pentimento_ipc::PrimitiveType::Cube,
            position: None,
            name: Some("Added after Open".into()),
        }));
    reopened.update();
    let ids: Vec<_> = reopened
        .world_mut()
        .query::<&crate::Selectable>()
        .iter(reopened.world())
        .map(|s| s.id.clone())
        .collect();
    assert!(ids.contains(&"object_82".into()));
    assert!(ids.contains(&"object_83".into()));
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
}

#[test]
fn finite_but_overflowing_pixels_camera_and_lighting_are_rejected_before_install() {
    let (mut app, _) = editor();
    let mut doc = document(&mut app);
    let canvas = doc
        .objects
        .iter_mut()
        .find_map(|o| o.canvas.as_mut())
        .unwrap();
    canvas.layers.layers[0].pixels[0][3] = f32::MAX;
    assert!(doc.prepare().is_err());
    let mut doc = document(&mut app);
    let mut lighting = pentimento_ipc::LightingSettings::default();
    lighting.sun_direction = [0.; 3];
    doc.lighting = Some(lighting);
    assert!(doc.prepare().is_err());
    let mut doc = document(&mut app);
    let mut view = ViewDocument::capture(&crate::OrbitCamera::default());
    view.target = [f32::MAX; 3];
    view.distance = f32::MAX;
    view.max_distance = f32::MAX;
    doc.view = Some(view);
    assert!(doc.prepare().is_err());
}

#[test]
fn missing_material_component_and_unsupported_feature_refuse_save() {
    let dir = OwnedDirectory::new();
    let path = dir.file("missing");
    let (mut app, target) = editor();
    let handle = app
        .world()
        .get::<MeshMaterial3d<StandardMaterial>>(target)
        .unwrap()
        .0
        .clone();
    app.world_mut()
        .entity_mut(target)
        .remove::<MeshMaterial3d<StandardMaterial>>();
    assert!(!save(&mut app, &path));
    assert!(!path.exists());
    assert!(
        app.world()
            .resource::<ProjectState>()
            .notice
            .as_ref()
            .unwrap()
            .contains("material component")
    );
    app.world_mut()
        .entity_mut(target)
        .insert(MeshMaterial3d(handle.clone()));
    app.world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&handle)
        .unwrap()
        .emissive = LinearRgba::RED;
    assert!(!save(&mut app, &path));
    assert!(!path.exists());
    assert!(
        app.world()
            .resource::<ProjectState>()
            .notice
            .as_ref()
            .unwrap()
            .contains("material feature")
    );
}

#[cfg(feature = "sculpting")]
#[test]
fn idle_active_sculpt_save_preserves_live_raw_render_and_reentered_authoring_identity() {
    let dir = OwnedDirectory::new();
    let path = dir.file("active-sculpt");
    let mut app = crate::sculpt_mode::sculpt_geometry_sync_tests::project_sculpt_app();
    app.init_resource::<Assets<Image>>()
        .init_resource::<PaintingResource>()
        .init_resource::<ProjectState>()
        .init_resource::<ProjectionTargets>();
    let h = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Sphere::new(1.).mesh().uv(16, 8));
    let mat = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::default());
    let entity = app
        .world_mut()
        .spawn((
            Mesh3d(h.clone()),
            MeshMaterial3d(mat),
            GlobalTransform::IDENTITY,
            Transform::default(),
            Name::new("Receiver"),
        ))
        .id();
    app.world_mut()
        .write_message(crate::SculptEvent::Enter { entity });
    app.update();
    assert!(app.world().resource::<crate::SculptState>().active);
    let chunks = app
        .world()
        .resource::<crate::sculpt_mode::SculptingData>()
        .chunked_mesh
        .as_ref()
        .unwrap()
        .clone();
    let live =
        MeshDocument::capture(app.world().resource::<Assets<Mesh>>().get(&h).unwrap()).unwrap();
    app.world_mut()
        .resource_mut::<Messages<crate::SculptEvent>>()
        .clear();
    assert!(
        save(&mut app, &path),
        "{:?}",
        app.world().resource::<ProjectState>().notice
    );
    let (mut fresh, _) = editor();
    assert!(open(&mut fresh, &path));
    let target = receiver(&mut fresh);
    let loaded = fresh.world().get::<ProjectSculptGeometry>(target).unwrap();
    assert!(loaded.mesh.same_authoritative_state(&chunks));
    let raw = fresh
        .world()
        .resource::<Assets<Mesh>>()
        .get(&fresh.world().get::<Mesh3d>(target).unwrap().0)
        .unwrap();
    assert!(live.matches(raw));
    assert!(open(&mut app, &path));
    let target = receiver(&mut app);
    app.world_mut()
        .write_message(crate::SculptEvent::Enter { entity: target });
    app.update();
    assert!(
        app.world()
            .resource::<crate::sculpt_mode::SculptingData>()
            .chunked_mesh
            .as_ref()
            .unwrap()
            .same_authoritative_state(&chunks)
    );
}

#[cfg(feature = "sculpting")]
fn sculpt_fixture() -> sculpting::ChunkedMesh {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let he = painting::half_edge::HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    sculpting::partition_mesh(
        &he,
        &sculpting::PartitionConfig {
            min_faces: 1,
            target_faces: 80,
            max_faces: 150,
        },
    )
}
#[cfg(feature = "sculpting")]
#[test]
fn sculpt_topology_uv_global_ids_counters_and_first_real_history_roundtrip() {
    let dir = OwnedDirectory::new();
    let path = dir.file("sculpt");
    let (mut app, target) = editor();
    let original = sculpt_fixture();
    assert!(original.chunk_count() > 1);
    let mesh = sculpting::merge_chunks(&original).mesh.to_bevy_mesh();
    let rendered = MeshDocument::capture(&mesh).unwrap();
    let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    app.world_mut().entity_mut(target).insert((
        Mesh3d(handle),
        ProjectSculptGeometry {
            mesh: original.clone(),
            rendered,
        },
    ));
    assert!(save(&mut app, &path));
    let (mut reopened, _) = editor();
    assert!(open(&mut reopened, &path));
    let target = receiver(&mut reopened);
    let mut geometry = reopened
        .world()
        .get::<ProjectSculptGeometry>(target)
        .unwrap()
        .mesh
        .clone();
    assert!(geometry.same_authoritative_state(&original));
    let preset = sculpting::BrushPreset {
        radius: 0.75,
        strength: 0.015,
        spacing: 0.,
        autosmooth: 0.,
        ..sculpting::BrushPreset::push()
    };
    let config = sculpting::PipelineConfig {
        tessellation_enabled: false,
        rebalance_after_stroke: false,
        ..default()
    };
    let mut pipeline = sculpting::SculptingPipeline::with_config(preset, config);
    pipeline.reset_history(7, &mut geometry, None).unwrap();
    assert_eq!(pipeline.history_status().undo_strokes, 0);
    let input = |x| sculpting::BrushInput {
        position: Vec3::new(x, 0., 1.),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 1,
    };
    pipeline.begin_stroke(7, input(0.));
    assert!(
        pipeline
            .process_input(input(0.01), &mut geometry)
            .rejected
            .is_none()
    );
    assert!(pipeline.end_stroke(&mut geometry).rejected.is_none());
    let after = geometry.clone();
    assert!(!after.same_authoritative_state(&original));
    assert!(pipeline.restore_history(&mut geometry, false).unwrap());
    assert!(geometry.same_authoritative_state(&original));
    assert!(pipeline.restore_history(&mut geometry, true).unwrap());
    assert!(geometry.same_authoritative_state(&after));
    assert!(pipeline.end_stroke(&mut geometry).packets.is_empty());
    let raw = MeshDocument::capture(
        reopened
            .world()
            .resource::<Assets<Mesh>>()
            .get(&reopened.world().get::<Mesh3d>(target).unwrap().0)
            .unwrap(),
    )
    .unwrap();
    let original_target = receiver(&mut app);
    assert!(
        raw.matches(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&app.world().get::<Mesh3d>(original_target).unwrap().0)
                .unwrap()
        )
    );
}

#[cfg(feature = "sculpting")]
#[test]
fn malformed_chunk_references_and_geometric_degeneracy_never_replace_document() {
    let dir = OwnedDirectory::new();
    let path = dir.file("bad-sculpt");
    let (mut app, target) = editor();
    let original = sculpt_fixture();
    let raw = sculpting::merge_chunks(&original).mesh.to_bevy_mesh();
    let rendered = MeshDocument::capture(&raw).unwrap();
    let h = app.world_mut().resource_mut::<Assets<Mesh>>().add(raw);
    app.world_mut().entity_mut(target).insert((
        Mesh3d(h),
        ProjectSculptGeometry {
            mesh: original,
            rendered,
        },
    ));
    let doc = document(&mut app);
    let pristine = serialize(&doc).unwrap();
    for kind in [0, 1, 2, 3, 4] {
        let mut value: serde_json::Value = serde_json::from_slice(&pristine).unwrap();
        let object = value["objects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|o| !o["sculpt"].is_null())
            .unwrap();
        if kind == 3 {
            object["mesh"]["positions"][0][0] = serde_json::json!(42.0);
        } else if kind == 4 {
            object["mesh"]["uv0"][0][0] = serde_json::json!(0.12345);
        }
        let sculpt = &mut object["sculpt"];
        if kind == 0 {
            sculpt["chunks"][0]["mesh"]["edges"][0]["next"] = u32::MAX.into();
        } else if kind == 1 {
            sculpt["next_original_vertex_id"] = u32::MAX.into();
        } else if kind == 2 {
            let p = sculpt["chunks"][0]["mesh"]["vertices"][1]["position"].clone();
            sculpt["chunks"][0]["mesh"]["vertices"][0]["position"] = p;
        }
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let assets = counts(&app);
        assert!(!open(&mut app, &path));
        assert_eq!(counts(&app), assets);
        assert_eq!(serialize(&document(&mut app)).unwrap(), pristine);
    }
}

fn new(app: &mut App, generation: u64, confirmed: bool) -> bool {
    command(
        app,
        ProjectCommand::New {
            expected_generation: generation.to_string(),
            confirm_discard: confirmed,
        },
    )
}

#[test]
fn new_cancel_stale_pending_and_active_refusal_preserve_owned_document_and_history() {
    let dir = OwnedDirectory::new();
    let path = dir.file("old");
    let (mut app, _) = editor();
    {
        let mut p = app.world_mut().resource_mut::<PaintingResource>();
        let p = p.get_pipeline_mut(0).unwrap();
        p.begin_stroke(0, 91, 0);
        p.stroke_to(1., 1., 1.);
        p.end_stroke();
        p.set_color([0., 1., 0., 1.]);
        p.begin_stroke(0, 92, 0);
        p.stroke_to(1., 1., 1.);
        p.end_stroke();
        assert!(p.undo());
        assert!(p.can_undo() && p.can_redo());
    }
    assert!(save(&mut app, &path));
    let before = serialize(&document(&mut app)).unwrap();
    let assets = counts(&app);
    let bytes = read_bounded(&path).unwrap();
    for (generation, confirmed) in [(0, false), (1, true)] {
        assert!(!new(&mut app, generation, confirmed));
        assert_eq!(serialize(&document(&mut app)).unwrap(), before);
        assert_eq!(counts(&app), assets);
        assert_eq!(
            app.world().resource::<ProjectState>().original.as_ref(),
            Some(&bytes)
        );
    }
    app.add_message::<crate::CanvasPlaneEvent>();
    app.world_mut()
        .write_message(crate::CanvasPlaneEvent::CreateInFrontOfCamera {
            width: 2,
            height: 2,
        });
    assert!(!new(&mut app, 0, true));
    clear_messages::<crate::CanvasPlaneEvent>(app.world_mut());
    let window = app.world_mut().spawn_empty().id();
    app.init_resource::<crate::FrontendScenePointerInput>();
    app.world_mut()
        .resource_mut::<crate::FrontendScenePointerInput>()
        .publish(
            window,
            vec![bevy::window::WindowEvent::MouseButtonInput(
                bevy::input::mouse::MouseButtonInput {
                    window,
                    button: MouseButton::Left,
                    state: bevy::input::ButtonState::Pressed,
                },
            )],
        );
    assert!(!new(&mut app, 0, true));
    app.world_mut()
        .resource_mut::<crate::FrontendScenePointerInput>()
        .clear();
    app.world_mut()
        .resource_mut::<PaintingResource>()
        .get_pipeline_mut(0)
        .unwrap()
        .begin_stroke(0, 92, 0);
    assert!(!new(&mut app, 0, true));
    assert!(
        app.world()
            .resource::<PaintingResource>()
            .get_pipeline(0)
            .unwrap()
            .is_stroking()
    );
    app.world_mut()
        .resource_mut::<PaintingResource>()
        .get_pipeline_mut(0)
        .unwrap()
        .cancel_stroke();
    assert_eq!(serialize(&document(&mut app)).unwrap(), before);
    assert!(
        app.world()
            .resource::<PaintingResource>()
            .get_pipeline(0)
            .unwrap()
            .can_undo()
    );
    assert_eq!(
        app.world().resource::<ProjectState>().path.as_ref(),
        Some(&path)
    );
    assert!(
        app.world()
            .resource::<PaintingResource>()
            .get_pipeline(0)
            .unwrap()
            .can_redo()
    );
    assert_eq!(read_bounded(&path).unwrap(), bytes);
    assert_eq!(project_generation(app.world()), 0);
}

#[test]
fn confirmed_new_resets_owner_defaults_history_and_identities_and_save_as_works() {
    let dir = OwnedDirectory::new();
    let old = dir.file("old");
    let fresh = dir.file("fresh");
    let (mut app, _) = editor();
    {
        let mut p = app.world_mut().resource_mut::<PaintingResource>();
        let p = p.get_pipeline_mut(0).unwrap();
        p.begin_stroke(0, 9, 0);
        p.stroke_to(1., 1., 1.);
        p.end_stroke();
        assert!(p.can_undo());
    }
    let camera = app
        .world_mut()
        .query_filtered::<Entity, With<crate::MainCamera>>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .entity_mut(camera)
        .insert(crate::OrbitCamera {
            distance: 37.,
            ..default()
        });
    app.init_resource::<crate::SceneLighting>()
        .init_resource::<crate::SceneAmbientOcclusion>();
    assert!(save(&mut app, &old));
    let bytes = read_bounded(&old).unwrap();
    // A blocked external file owner must not prevent starting a separate document.
    app.world_mut().resource_mut::<ProjectState>().blocked = true;
    assert!(new(&mut app, 0, true));
    assert_eq!(project_generation(app.world()), 1);
    let state = app.world().resource::<ProjectState>();
    assert!(state.path.is_none() && state.original.is_none() && !state.blocked);
    assert!(document_entities(app.world_mut()).is_empty());
    assert_eq!(
        app.world()
            .get::<crate::OrbitCamera>(camera)
            .unwrap()
            .distance,
        crate::OrbitCamera::default().distance
    );
    let doc = document(&mut app);
    assert!(doc.objects.is_empty());
    assert_eq!(
        (
            doc.next_object_id,
            doc.next_plane_id,
            doc.next_mesh_id,
            doc.next_stroke_id,
            doc.object_counter
        ),
        (1, 0, 0, 0, 0)
    );
    assert!(
        app.world()
            .resource::<PaintingResource>()
            .get_pipeline(0)
            .is_none()
    );
    let messages = &app.world().resource::<OutboundUiMessages>().messages;
    assert!(messages.iter().any(|m| matches!(
        m,
        BevyToUi::PaintBrushStateChanged {
            can_undo: false,
            can_redo: false,
            ..
        }
    )));
    #[cfg(feature = "sculpting")]
    assert!(messages.iter().any(|m| matches!(
        m,
        BevyToUi::SculptHistoryChanged {
            undo_strokes: 0,
            redo_strokes: 0,
            active: false,
            ..
        }
    )));
    assert!(!new(&mut app, 0, true)); // an old still-open confirmation cannot replace again
    assert!(save(&mut app, &fresh));
    assert_eq!(read_bounded(&old).unwrap(), bytes);
    assert!(
        parse(&read_bounded(&fresh).unwrap())
            .unwrap()
            .objects
            .is_empty()
    );
    assert!(open(&mut app, &fresh));
    assert_eq!(project_generation(app.world()), 2);
}

#[test]
fn new_preflight_failure_keeps_document_owner_generation_and_assets() {
    let (mut app, _) = editor();
    let before = serialize(&document(&mut app)).unwrap();
    let assets = app.world_mut().remove_resource::<Assets<Image>>().unwrap();
    assert!(!new(&mut app, 0, true));
    assert_eq!(project_generation(app.world()), 0);
    app.world_mut().insert_resource(assets);
    assert_eq!(serialize(&document(&mut app)).unwrap(), before);
}

#[cfg(feature = "sculpting")]
#[test]
fn new_cancel_preserves_genuine_sculpt_redo_and_confirmed_new_releases_geometry_and_history() {
    let (mut app, target) = editor();
    let mut geometry = sculpt_fixture();
    let original = geometry.clone();
    let mut pipeline = sculpting::SculptingPipeline::with_config(
        sculpting::BrushPreset {
            radius: 0.75,
            strength: 0.015,
            spacing: 0.,
            autosmooth: 0.,
            ..sculpting::BrushPreset::push()
        },
        sculpting::PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..default()
        },
    );
    pipeline.reset_history(7, &mut geometry, None).unwrap();
    let input = |x| sculpting::BrushInput {
        position: Vec3::new(x, 0., 1.),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 1,
    };
    pipeline.begin_stroke(7, input(0.));
    assert!(
        pipeline
            .process_input(input(0.01), &mut geometry)
            .rejected
            .is_none()
    );
    assert!(pipeline.end_stroke(&mut geometry).rejected.is_none());
    assert_eq!(pipeline.history_status().undo_strokes, 1);
    assert!(pipeline.restore_history(&mut geometry, false).unwrap());
    assert_eq!(pipeline.history_status().redo_strokes, 1);
    assert!(geometry.same_authoritative_state(&original));
    let render = sculpting::merge_chunks(&geometry).mesh.to_bevy_mesh();
    let rendered = MeshDocument::capture(&render).unwrap();
    let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(render);
    app.world_mut().entity_mut(target).insert((
        Mesh3d(handle),
        ProjectSculptGeometry {
            mesh: geometry.clone(),
            rendered,
        },
    ));
    let chunk = app.world_mut().spawn_empty().id();
    *app.world_mut()
        .resource_mut::<crate::sculpt_mode::SculptingData>() = crate::sculpt_mode::SculptingData {
        chunked_mesh: Some(geometry),
        pipeline: Some(pipeline),
        chunk_entities: vec![chunk],
        ..default()
    };
    assert!(!new(&mut app, 0, false));
    let sculpt = app.world().resource::<crate::sculpt_mode::SculptingData>();
    assert_eq!(
        sculpt
            .pipeline
            .as_ref()
            .unwrap()
            .history_status()
            .redo_strokes,
        1
    );
    assert!(
        sculpt
            .chunked_mesh
            .as_ref()
            .unwrap()
            .same_authoritative_state(&original)
    );
    assert!(new(&mut app, 0, true));
    let sculpt = app.world().resource::<crate::sculpt_mode::SculptingData>();
    assert!(
        sculpt.pipeline.is_none()
            && sculpt.chunked_mesh.is_none()
            && sculpt.chunk_entities.is_empty()
    );
    assert!(app.world().get_entity(chunk).is_err() && app.world().get_entity(target).is_err());
}
