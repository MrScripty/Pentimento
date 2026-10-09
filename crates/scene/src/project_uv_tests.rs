use super::*;
use crate::{MeshPaintEvent, MeshPaintTexture, MeshPaintingResource, PaintableMesh};
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use painting::{MeshHit, MeshStorageMode};
use std::sync::atomic::{AtomicU64, Ordering};
static OWNED: AtomicU64 = AtomicU64::new(0);
struct Owned(PathBuf);
impl Owned {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "pentimento-directuv-{}-{}",
            std::process::id(),
            OWNED.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}.pentimento.json"))
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn editor() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(20),
        ))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<PaintingResource>()
        .init_resource::<OutboundUiMessages>()
        .init_resource::<ProjectState>()
        .add_message::<MeshPaintEvent>()
        .add_plugins(crate::MeshPaintingSystemPlugin);
    #[cfg(feature = "sculpting")]
    app.init_resource::<crate::SculptState>()
        .init_resource::<crate::sculpt_mode::SculptingData>();
    let mut brush = painting::BrushPreset::default();
    brush.base_size = 2.;
    brush.min_size = 2.;
    brush.max_size = 2.;
    brush.opacity = 0.5;
    brush.hardness = 1.;
    let mut r = app.world_mut().resource_mut::<MeshPaintingResource>();
    r.set_brush_preset(brush);
    r.set_brush_color([0.9, 0.1, 0.6, 0.75]);
    app
}
fn add(app: &mut App, id: u32, textured: bool) -> Entity {
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Rectangle::new(2., 2.));
    let image = textured.then(|| {
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new(
                Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                (0..64).flat_map(|i| [i as u8, 128, 200, 175]).collect(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            ))
    });
    let material = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgba(0.2, 0.4, 0.7, 0.8),
            base_color_texture: image,
            ..default()
        });
    app.world_mut()
        .spawn((
            Name::new(format!("UV{id}")),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            PaintableMesh {
                mesh_id: id,
                storage_mode: MeshStorageMode::UvAtlas { resolution: (8, 8) },
            },
        ))
        .id()
}
fn named(app: &mut App, id: u32) -> Entity {
    app.world_mut()
        .query::<(Entity, &Name)>()
        .iter(app.world())
        .find(|(_, n)| n.as_str() == format!("UV{id}"))
        .unwrap()
        .0
}
fn hit(uv: Option<Vec2>) -> MeshHit {
    MeshHit {
        world_pos: Vec3::ZERO,
        face_id: 0,
        barycentric: Vec3::new(0.2, 0.3, 0.5),
        normal: Vec3::Z,
        tangent: Vec3::X,
        bitangent: Vec3::Y,
        uv,
    }
}
fn event(app: &mut App, e: MeshPaintEvent) {
    app.world_mut().write_message(e);
    app.update();
}
fn stroke(app: &mut App, e: Entity, id: u32, uv: Option<Vec2>, cancel: bool) {
    event(
        app,
        MeshPaintEvent::StrokeStart {
            mesh_entity: e,
            mesh_id: id,
            hit: hit(uv),
            stroke_id: 17,
        },
    );
    event(
        app,
        if cancel {
            MeshPaintEvent::StrokeCancel
        } else {
            MeshPaintEvent::StrokeEnd
        },
    );
}
fn settle(app: &mut App) {
    for _ in 0..3 {
        app.update();
    }
}
fn file_command(app: &mut App, c: ProjectCommand) -> bool {
    crate::dispatch_brush_ui_command(
        app.world_mut(),
        &pentimento_ipc::UiToBevy::ProjectCommand(c),
    );
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
fn save(app: &mut App, p: &Path) -> bool {
    file_command(
        app,
        ProjectCommand::Save {
            path: p.to_string_lossy().into_owned(),
        },
    )
}
fn open(app: &mut App, p: &Path) -> bool {
    file_command(
        app,
        ProjectCommand::Open {
            path: p.to_string_lossy().into_owned(),
        },
    )
}
fn pixels(app: &App, id: u32) -> Vec<[f32; 4]> {
    app.world()
        .resource::<MeshPaintingResource>()
        .get_uv_surface(id)
        .unwrap()
        .atlas
        .surface()
        .pixels()
        .to_vec()
}
fn image(app: &App, e: Entity) -> Vec<u8> {
    let texture = app.world().get::<MeshPaintTexture>(e).unwrap();
    let material = app
        .world()
        .get::<MeshMaterial3d<StandardMaterial>>(e)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<Assets<StandardMaterial>>()
            .get(&material.0)
            .unwrap()
            .base_color_texture,
        Some(texture.image_handle.clone())
    );
    app.world()
        .resource::<Assets<Image>>()
        .get(&texture.image_handle)
        .unwrap()
        .data
        .clone()
        .unwrap()
}

#[test]
fn owned_direct_uv_roundtrip_keeps_two_surfaces_original_bytes_brush_and_real_history() {
    let owned = Owned::new();
    let path = owned.file("painted");
    let mut app = editor();
    let a = add(&mut app, 12, true);
    let b = add(&mut app, 44, false);
    app.update();
    stroke(&mut app, a, 12, Some(Vec2::new(0.3125, 0.6875)), false);
    stroke(&mut app, b, 44, Some(Vec2::new(0.6875, 0.3125)), false);
    settle(&mut app);
    let expected_a = image(&app, a);
    let expected_b = image(&app, b);
    let raw_a = pixels(&app, 12);
    let raw_b = pixels(&app, 44);
    assert_ne!(expected_a, expected_b);
    assert_ne!(raw_a, raw_b);
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        1
    );
    assert!(
        save(&mut app, &path),
        "{:?}",
        app.world().resource::<ProjectState>().notice
    );
    let expected = capture(app.world_mut()).unwrap().0;
    assert_eq!(expected.version, 2);
    let mut reopened = editor();
    assert!(open(&mut reopened, &path));
    let a = named(&mut reopened, 12);
    let b = named(&mut reopened, 44);
    assert_eq!(image(&reopened, a), expected_a);
    assert_eq!(image(&reopened, b), expected_b);
    settle(&mut reopened);
    assert_eq!(image(&reopened, a), expected_a);
    assert_eq!(pixels(&reopened, 12), raw_a);
    assert_eq!(pixels(&reopened, 44), raw_b);
    assert_eq!(
        serialize(&capture(reopened.world_mut()).unwrap().0).unwrap(),
        serialize(&expected).unwrap()
    );
    assert_eq!(
        reopened
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        0
    );
    for _ in 0..3 {
        let before = pixels(&reopened, 12);
        let before_image = image(&reopened, a);
        stroke(&mut reopened, a, 12, Some(Vec2::new(0.5625, 0.4375)), false);
        let after = pixels(&reopened, 12);
        let after_image = image(&reopened, a);
        assert_ne!(before, after);
        assert!(crate::undo_mesh_paint(reopened.world_mut(), a));
        assert!(
            !save(&mut reopened, &path),
            "stale direct Image cannot be saved before upload"
        );
        settle(&mut reopened);
        assert_eq!(pixels(&reopened, 12), before);
        assert_eq!(image(&reopened, a), before_image);
        assert!(crate::redo_mesh_paint(reopened.world_mut(), a));
        settle(&mut reopened);
        assert_eq!(pixels(&reopened, 12), after);
        assert_eq!(image(&reopened, a), after_image);
        assert_eq!(pixels(&reopened, 44), raw_b);
    }
    assert!(save(&mut reopened, &path));
    let final_image = image(&reopened, a);
    let mut again = editor();
    assert!(open(&mut again, &path));
    let a = named(&mut again, 12);
    settle(&mut again);
    assert_eq!(image(&again, a), final_image);
    std::fs::write(&path, b"external owner").unwrap();
    assert!(!save(&mut reopened, &path));
    assert_eq!(std::fs::read(&path).unwrap(), b"external owner");
    assert!(save(&mut reopened, &owned.file("recovered")));
}

#[test]
fn direct_transactions_cancel_noop_active_conflict_and_move_ownership_are_real() {
    let owned = Owned::new();
    let path = owned.file("idle");
    let mut app = editor();
    let a = add(&mut app, 12, true);
    let b = add(&mut app, 44, false);
    app.update();
    let blank_b = pixels(&app, 44);
    stroke(&mut app, a, 12, Some(Vec2::splat(0.3)), false);
    settle(&mut app);
    assert!(save(&mut app, &path));
    let saved = std::fs::read(&path).unwrap();
    let before = pixels(&app, 12);
    event(
        &mut app,
        MeshPaintEvent::StrokeStart {
            mesh_entity: a,
            mesh_id: 12,
            hit: hit(Some(Vec2::splat(0.6))),
            stroke_id: 18,
        },
    );
    event(
        &mut app,
        MeshPaintEvent::StrokeMove {
            hit: hit(Some(Vec2::splat(0.8))),
            pressure: 1.,
            speed: 0.,
        },
    );
    assert_eq!(pixels(&app, 44), blank_b);
    assert!(!save(&mut app, &path));
    assert!(!open(&mut app, &path));
    assert!(!crate::undo_mesh_paint(app.world_mut(), a));
    event(&mut app, MeshPaintEvent::StrokeCancel);
    settle(&mut app);
    assert_eq!(pixels(&app, 12), before);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(crate::undo_mesh_paint(app.world_mut(), a));
    settle(&mut app);
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .redo_count(12),
        1
    );
    stroke(&mut app, a, 12, None, false);
    stroke(&mut app, a, 12, Some(Vec2::splat(0.5)), true);
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .redo_count(12),
        1
    );
    stroke(&mut app, b, 44, Some(Vec2::splat(0.5)), false);
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .redo_count(12),
        0,
        "new accepted stroke clears redo"
    );
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .undo_count(44),
        1
    );
    app.world_mut()
        .resource_mut::<MeshPaintingResource>()
        .get_uv_surface_mut(44)
        .unwrap()
        .atlas
        .surface_mut()
        .pixels_mut()[0] = [0.5; 4];
    assert!(
        !crate::undo_mesh_paint(app.world_mut(), b),
        "external source pixels must not be overwritten by history"
    );
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .get_uv_surface(44)
            .unwrap()
            .atlas
            .surface()
            .pixels()[0],
        [0.5; 4]
    );
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .undo_count(44),
        0,
        "conflicting history is invalidated"
    );
}

#[test]
fn v1_read_and_required_v2_fields_budgets_baselines_and_mixed_ptex_are_explicit() {
    let owned = Owned::new();
    let path = owned.file("good");
    let mut app = editor();
    let a = add(&mut app, 12, true);
    app.update();
    stroke(&mut app, a, 12, Some(Vec2::splat(0.5)), false);
    settle(&mut app);
    assert!(save(&mut app, &path));
    let good = capture(app.world_mut()).unwrap().0;
    let baseline = pixels(&app, 12);
    let count = app.world().resource::<Assets<Mesh>>().len();
    for kind in 0..7 {
        let mut value = serde_json::to_value(&good).unwrap();
        match kind {
            0 => {
                value["objects"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("mesh_uv");
            }
            1 => value["objects"][0]["mesh_uv"]["width"] = 99999.into(),
            2 => value["objects"][0]["mesh_uv"]["pixels"][0][0] = 2.into(),
            3 => value["objects"][0]["mesh_uv"]["original_linear_color"][0] = 0.99.into(),
            4 => value["objects"][0]["mesh_uv"]["display_bound"] = false.into(),
            5 => value["version"] = 1.into(),
            _ => value["mesh_brush"] = serde_json::Value::Null,
        }
        let bad = owned.file(&format!("bad{kind}"));
        std::fs::write(&bad, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(!open(&mut app, &bad), "kind{kind}");
        assert_eq!(pixels(&app, 12), baseline);
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), count);
    }
    let mut old = good.clone();
    old.version = 1;
    old.mesh_brush = None;
    old.objects[0].mesh_uv = None;
    assert!(
        parse(&serialize(&old).unwrap()).is_ok(),
        "old unpainted v1 component files retain read compatibility"
    );
    let old_path = owned.file("v1");
    std::fs::write(&old_path, serialize(&old).unwrap()).unwrap();
    let mut legacy = editor();
    assert!(open(&mut legacy, &old_path));
    settle(&mut legacy);
    let legacy_a = named(&mut legacy, 12);
    assert!(
        pixels(&legacy, 12)
            .iter()
            .flatten()
            .all(|p| p.to_bits() == 0)
    );
    stroke(&mut legacy, legacy_a, 12, Some(Vec2::splat(0.4)), false);
    assert!(crate::undo_mesh_paint(legacy.world_mut(), legacy_a));
    settle(&mut legacy);
    assert!(crate::redo_mesh_paint(legacy.world_mut(), legacy_a));
    settle(&mut legacy);
    assert!(save(&mut legacy, &owned.file("v2-migrated")));
    let pixels_len = good.objects[0].mesh_uv.as_ref().unwrap().pixels.len();
    let mut huge = good.clone();
    for _ in 0..256 {
        let mut o = huge.objects[0].clone();
        o.id = huge.next_object_id;
        huge.next_object_id += 1;
        let id = huge.next_mesh_id;
        huge.next_mesh_id += 1;
        o.paintable.as_mut().unwrap().0 = id;
        o.mesh_uv.as_mut().unwrap().pixels = vec![[0.1; 4]; pixels_len];
        huge.objects.push(o);
    }
    assert!(huge.prepare().is_err());
    let mut pixel_huge = good.clone();
    pixel_huge.objects.clear();
    for id in 0..5 {
        let mut o = good.objects[0].clone();
        o.id = id + 1;
        o.paintable = Some((id as u32, StorageDocument::Uv(1024, 1024)));
        o.material.texture = None;
        let uv = o.mesh_uv.as_mut().unwrap();
        uv.width = 1024;
        uv.height = 1024;
        uv.pixels = vec![[0.; 4]; 1024 * 1024];
        pixel_huge.objects.push(o);
    }
    pixel_huge.next_object_id = 6;
    pixel_huge.next_mesh_id = 5;
    assert!(pixel_huge.validate().unwrap_err().contains("budget"));
    app.world_mut()
        .entity_mut(a)
        .insert(crate::ProjectionTarget::uv_atlas((8, 8)));
    app.world_mut().init_resource::<ProjectionTargets>();
    app.world_mut()
        .resource_mut::<ProjectionTargets>()
        .restore_document_layers(a, (8, 8), vec![(0, vec![[0.5; 4]; 64])]);
    assert!(!save(&mut app, &owned.file("mixed")));
    app.world_mut().entity_mut(a).insert(PaintableMesh {
        mesh_id: 12,
        storage_mode: MeshStorageMode::Ptex { face_resolution: 8 },
    });
    app.world_mut()
        .resource_mut::<MeshPaintingResource>()
        .get_or_create_ptex_surface(12, 8)
        .get_or_create_face(0)
        .set_pixel(1, 1, [0.5; 4]);
    assert!(!save(&mut app, &owned.file("ptex")));
    assert!(
        app.world()
            .resource::<ProjectState>()
            .notice
            .as_ref()
            .unwrap()
            .contains("PTex")
    );
}

fn appearance(app: &App, e: Entity) -> (Color, Option<Handle<Image>>) {
    let h = &app
        .world()
        .get::<MeshMaterial3d<StandardMaterial>>(e)
        .unwrap()
        .0;
    let m = app
        .world()
        .resource::<Assets<StandardMaterial>>()
        .get(h)
        .unwrap();
    (m.base_color, m.base_color_texture.clone())
}

#[test]
fn first_cancel_and_undo_restore_original_tinted_texture_binding() {
    let owned = Owned::new();
    let mut app = editor();
    let a = add(&mut app, 12, true);
    app.update();
    let original = appearance(&app, a);
    assert_ne!(original.0, Color::WHITE);
    assert!(original.1.is_some());
    stroke(&mut app, a, 12, Some(Vec2::splat(0.4)), true);
    settle(&mut app);
    assert_eq!(appearance(&app, a), original);
    assert!(!app.world().get::<MeshPaintTexture>(a).unwrap().has_paint);
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        0
    );
    assert!(pixels(&app, 12).iter().flatten().all(|p| p.to_bits() == 0));
    stroke(&mut app, a, 12, Some(Vec2::splat(0.4)), false);
    let painted = image(&app, a);
    assert!(crate::undo_mesh_paint(app.world_mut(), a));
    settle(&mut app);
    assert_eq!(appearance(&app, a), original);
    assert!(save(&mut app, &owned.file("undone")));
    let mut reopened = editor();
    assert!(open(&mut reopened, &owned.file("undone")));
    let e = named(&mut reopened, 12);
    settle(&mut reopened);
    assert_eq!(appearance(&reopened, e).0, original.0);
    assert!(
        !reopened
            .world()
            .get::<MeshPaintTexture>(e)
            .unwrap()
            .has_paint
    );
    assert!(crate::redo_mesh_paint(app.world_mut(), a));
    settle(&mut app);
    assert_eq!(image(&app, a), painted);
}

#[test]
fn exact_transparent_float_bits_and_seam_metadata_survive_owned_reopen() {
    let owned = Owned::new();
    let mut app = editor();
    let a = add(&mut app, 12, true);
    app.update();
    stroke(&mut app, a, 12, Some(Vec2::splat(0.6)), false);
    {
        let mut r = app.world_mut().resource_mut::<MeshPaintingResource>();
        let s = r.get_uv_surface_mut(12).unwrap();
        s.seam_padding = 3;
        s.atlas.surface_mut().pixels_mut()[0] =
            [-0.0, f32::from_bits(1), f32::from_bits(0x3eaaaaab), 0.];
        s.atlas.mark_region_dirty(0, 0, 8, 8);
    }
    settle(&mut app);
    let raw = pixels(&app, 12);
    let bound = image(&app, a);
    assert!(save(&mut app, &owned.file("exact")));
    let mut reopened = editor();
    assert!(open(&mut reopened, &owned.file("exact")));
    let e = named(&mut reopened, 12);
    settle(&mut reopened);
    assert!(crate::mesh_uv_history::same_pixels(
        &raw,
        &pixels(&reopened, 12)
    ));
    assert_eq!(image(&reopened, e), bound);
    assert_eq!(
        reopened
            .world()
            .resource::<MeshPaintingResource>()
            .get_uv_surface(12)
            .unwrap()
            .seam_padding,
        3
    );
}

#[test]
fn external_derived_descriptors_and_original_bytes_refuse_save_and_history() {
    let owned = Owned::new();
    for case in 0..6 {
        let mut app = editor();
        let a = add(&mut app, 12, true);
        app.update();
        stroke(&mut app, a, 12, Some(Vec2::splat(0.4)), false);
        settle(&mut app);
        let path = owned.file(&format!("descriptor{case}"));
        assert!(save(&mut app, &path));
        let saved = std::fs::read(&path).unwrap();
        let raw = pixels(&app, 12);
        let texture = app.world().get::<MeshPaintTexture>(a).unwrap();
        let h = if case == 5 {
            texture.original_texture.as_ref().unwrap().clone()
        } else {
            texture.image_handle.clone()
        };
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let i = images.get_mut(&h).unwrap();
        match case {
            0 => i.texture_descriptor.format = TextureFormat::Bgra8UnormSrgb,
            1 => i.sampler = bevy::image::ImageSampler::nearest(),
            2 => i.texture_descriptor.mip_level_count = 2,
            3 => i.texture_descriptor.size.width = 4,
            4 => i.data = None,
            _ => i.data.as_mut().unwrap()[0] = 250,
        }
        settle(&mut app);
        assert!(!save(&mut app, &path), "case{case}");
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert!(!crate::undo_mesh_paint(app.world_mut(), a), "case{case}");
        assert!(crate::mesh_uv_history::same_pixels(&pixels(&app, 12), &raw));
    }
}

#[test]
fn geometry_storage_entity_and_live_external_surface_edits_invalidate_history() {
    for case in 0..5 {
        let mut app = editor();
        let a = add(&mut app, 12, false);
        app.update();
        stroke(&mut app, a, 12, Some(Vec2::splat(0.4)), false);
        settle(&mut app);
        assert_eq!(
            app.world()
                .resource::<MeshPaintingResource>()
                .undo_count(12),
            1
        );
        let before = pixels(&app, 12);
        match case {
            0 => {
                let h = app.world().get::<Mesh3d>(a).unwrap().0.clone();
                let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
                meshes
                    .get_mut(&h)
                    .unwrap()
                    .insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.1, 0.2]; 4]);
            }
            1 => {
                app.world_mut().entity_mut(a).insert(PaintableMesh {
                    mesh_id: 12,
                    storage_mode: MeshStorageMode::UvAtlas { resolution: (4, 4) },
                });
            }
            2 => {
                app.world_mut().despawn(a);
            }
            3 => {
                let _ = add(&mut app, 12, false);
            }
            _ => {
                let _ = app
                    .world_mut()
                    .resource_mut::<MeshPaintingResource>()
                    .get_or_create_uv_surface(12, 8, 8);
            }
        }
        assert!(!crate::undo_mesh_paint(app.world_mut(), a), "case{case}");
        assert!(crate::mesh_uv_history::same_pixels(
            &pixels(&app, 12),
            &before
        ));
        assert_eq!(
            app.world()
                .resource::<MeshPaintingResource>()
                .undo_count(12),
            0
        );
    }
    let mut app = editor();
    let a = add(&mut app, 12, false);
    app.update();
    event(
        &mut app,
        MeshPaintEvent::StrokeStart {
            mesh_entity: a,
            mesh_id: 12,
            hit: hit(Some(Vec2::splat(0.3))),
            stroke_id: 9,
        },
    );
    app.world_mut()
        .resource_mut::<MeshPaintingResource>()
        .get_uv_surface_mut(12)
        .unwrap()
        .atlas
        .surface_mut()
        .pixels_mut()[0] = [0.7; 4];
    let external = pixels(&app, 12);
    event(&mut app, MeshPaintEvent::StrokeCancel);
    assert!(crate::mesh_uv_history::same_pixels(
        &pixels(&app, 12),
        &external
    ));
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        0
    );
    // Multiple synchronous actions must not hide a mesh mutation in one tick.
    let mut synchronous = editor();
    let a = add(&mut synchronous, 12, false);
    synchronous.update();
    stroke(&mut synchronous, a, 12, Some(Vec2::splat(0.3)), false);
    assert!(crate::undo_mesh_paint(synchronous.world_mut(), a));
    let h = synchronous.world().get::<Mesh3d>(a).unwrap().0.clone();
    synchronous
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&h)
        .unwrap()
        .insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.2, 0.4]; 4]);
    assert!(!crate::redo_mesh_paint(synchronous.world_mut(), a));

    // An external display-byte edit during a live stroke must survive Cancel.
    let mut display = editor();
    let a = add(&mut display, 12, true);
    display.update();
    event(
        &mut display,
        MeshPaintEvent::StrokeStart {
            mesh_entity: a,
            mesh_id: 12,
            hit: hit(Some(Vec2::splat(0.3))),
            stroke_id: 22,
        },
    );
    let h = display
        .world()
        .get::<MeshPaintTexture>(a)
        .unwrap()
        .image_handle
        .clone();
    display
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .get_mut(&h)
        .unwrap()
        .data
        .as_mut()
        .unwrap()[0] = 249;
    let external = display
        .world()
        .resource::<Assets<Image>>()
        .get(&h)
        .unwrap()
        .data
        .clone();
    event(&mut display, MeshPaintEvent::StrokeCancel);
    assert_eq!(
        display
            .world()
            .resource::<Assets<Image>>()
            .get(&h)
            .unwrap()
            .data,
        external
    );
    assert!(
        display
            .world()
            .resource::<MeshPaintingResource>()
            .history_conflicted(12)
    );
    assert_eq!(
        display
            .world()
            .resource::<MeshPaintingResource>()
            .undo_count(12),
        0
    );
}

#[test]
fn production_transactions_enforce_retained_pending_and_eviction_budgets() {
    let mut app = editor();
    let a = add(&mut app, 12, false);
    app.world_mut().entity_mut(a).insert(PaintableMesh {
        mesh_id: 12,
        storage_mode: MeshStorageMode::UvAtlas {
            resolution: (512, 512),
        },
    });
    app.update();
    for n in 0..9 {
        event(
            &mut app,
            MeshPaintEvent::StrokeStart {
                mesh_entity: a,
                mesh_id: 12,
                hit: hit(Some(Vec2::new(0.1 + 0.08 * n as f32, 0.5))),
                stroke_id: n,
            },
        );
        let r = app.world().resource::<MeshPaintingResource>();
        assert_eq!(r.pending_history_bytes(), 4 * 1024 * 1024);
        assert!(r.history_bytes() <= crate::mesh_uv_history::MAX_HISTORY_BYTES);
        assert!(r.history_bytes() + r.pending_history_bytes() <= 96 * 1024 * 1024);
        event(&mut app, MeshPaintEvent::StrokeEnd);
    }
    let r = app.world().resource::<MeshPaintingResource>();
    assert_eq!(r.history_bytes(), 64 * 1024 * 1024);
    assert_eq!(r.pending_history_bytes(), 0);
    assert_eq!(r.undo_count(12), 8);
    assert_eq!(r.evicted_history_strokes(), 1);
    assert!(crate::undo_mesh_paint(app.world_mut(), a));
    settle(&mut app);
    let before = pixels(&app, 12);
    stroke(&mut app, a, 12, None, false);
    stroke(&mut app, a, 12, Some(Vec2::splat(0.8)), true);
    assert!(crate::mesh_uv_history::same_pixels(
        &pixels(&app, 12),
        &before
    ));
    let r = app.world().resource::<MeshPaintingResource>();
    assert_eq!(r.redo_count(12), 1);
    assert_eq!(r.evicted_history_strokes(), 1);
    assert_eq!(r.history_bytes(), 64 * 1024 * 1024);
    assert!(crate::redo_mesh_paint(app.world_mut(), a));
    settle(&mut app);
}

#[test]
fn metadata_cap_and_oversize_stroke_admission_are_real() {
    let mut app = editor();
    let a = add(&mut app, 12, false);
    app.update();
    for n in 0..129 {
        let index = n % 64;
        stroke(
            &mut app,
            a,
            12,
            Some(Vec2::new(
                (index % 8) as f32 / 8. + 0.0625,
                (index / 8) as f32 / 8. + 0.0625,
            )),
            false,
        );
    }
    let r = app.world().resource::<MeshPaintingResource>();
    assert_eq!(r.undo_count(12), 128);
    assert_eq!(r.evicted_history_strokes(), 1);
    assert_eq!(r.history_bytes(), 128 * 64 * 32);
    let mut large = editor();
    let b = add(&mut large, 44, false);
    large.world_mut().entity_mut(b).insert(PaintableMesh {
        mesh_id: 44,
        storage_mode: MeshStorageMode::UvAtlas {
            resolution: (2048, 2048),
        },
    });
    large.update();
    stroke(&mut large, b, 44, Some(Vec2::splat(0.5)), false);
    let r = large.world().resource::<MeshPaintingResource>();
    assert_eq!(r.pending_history_bytes(), 0);
    assert_eq!(r.history_bytes(), 0);
    assert!(!r.has_active_stroke());
    assert!(
        r.get_uv_surface(44)
            .unwrap()
            .atlas
            .surface()
            .pixels()
            .iter()
            .flatten()
            .all(|v| v.to_bits() == 0)
    );
}

#[test]
fn invalid_dabs_and_rejected_end_preserve_redo_and_baseline() {
    let mut app = editor();
    let a = add(&mut app, 12, true);
    app.update();
    stroke(&mut app, a, 12, Some(Vec2::splat(0.4)), false);
    assert!(crate::undo_mesh_paint(app.world_mut(), a));
    settle(&mut app);
    let original = appearance(&app, a);
    let raw = pixels(&app, 12);
    stroke(&mut app, a, 12, Some(Vec2::new(f32::NAN, 0.4)), false);
    app.world_mut()
        .resource_mut::<MeshPaintingResource>()
        .set_brush_color([f32::NAN, 0.1, 0.2, 0.75]);
    stroke(&mut app, a, 12, Some(Vec2::splat(0.4)), false);
    settle(&mut app);
    assert!(crate::mesh_uv_history::same_pixels(&raw, &pixels(&app, 12)));
    assert_eq!(appearance(&app, a), original);
    assert_eq!(
        app.world()
            .resource::<MeshPaintingResource>()
            .redo_count(12),
        1
    );
}

#[test]
fn unrelated_image_change_during_pending_undo_is_an_explicit_conservative_conflict() {
    let owned = Owned::new();
    let mut app = editor();
    let a = add(&mut app, 12, false);
    app.update();
    stroke(&mut app, a, 12, Some(Vec2::splat(0.4)), false);
    settle(&mut app);
    let path = owned.file("baseline");
    assert!(save(&mut app, &path));
    let bytes = std::fs::read(&path).unwrap();
    assert!(crate::undo_mesh_paint(app.world_mut(), a));
    let h = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_fill(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[13, 29, 47, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        ));
    settle(&mut app);
    assert_eq!(
        app.world()
            .resource::<Assets<Image>>()
            .get(&h)
            .unwrap()
            .data
            .as_ref()
            .unwrap(),
        &[13, 29, 47, 255].repeat(4)
    );
    assert!(
        app.world()
            .resource::<MeshPaintingResource>()
            .history_conflicted(12)
    );
    assert!(!crate::redo_mesh_paint(app.world_mut(), a));
    assert!(!save(&mut app, &path));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(open(&mut app, &path));
    let a = named(&mut app, 12);
    settle(&mut app);
    assert!(
        !app.world()
            .resource::<MeshPaintingResource>()
            .history_conflicted(12)
    );
    stroke(&mut app, a, 12, Some(Vec2::splat(0.7)), false);
    assert!(crate::undo_mesh_paint(app.world_mut(), a));
}
