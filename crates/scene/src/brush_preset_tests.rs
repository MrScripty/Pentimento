use super::*;
use crate::brush_ui::dispatch_brush_ui_command;
use pentimento_ipc::UiToBevy;

#[test]
fn identical_brushes_keep_the_explicitly_saved_or_restored_identity() {
    let fixture = Fixture::new();
    let mut world = fixture.world();
    for name in ["First name", "Second name"] {
        paint(
            &mut world,
            PaintCommand::SaveBrushPreset { name: name.into() },
        );
    }
    assert!(matches!(
        message(&world),
        Some(BevyToUi::SavedBrushPresetsChanged {
            selected_paint: Some(2),
            ..
        })
    ));
    paint(
        &mut world,
        PaintCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    assert!(matches!(
        message(&world),
        Some(BevyToUi::SavedBrushPresetsChanged {
            selected_paint: Some(1),
            ..
        })
    ));
    paint(
        &mut world,
        PaintCommand::SelectSavedBrushPreset { preset_id: 2 },
    );
    assert!(matches!(
        message(&world),
        Some(BevyToUi::SavedBrushPresetsChanged {
            selected_paint: Some(2),
            ..
        })
    ));
    paint(&mut world, PaintCommand::SetBrushSize { size: 70. });
    assert!(matches!(
        message(&world),
        Some(BevyToUi::SavedBrushPresetsChanged {
            selected_paint: None,
            ..
        })
    ));
}

#[test]
fn unavailable_storage_and_preserved_other_mode_data_do_not_report_false_saves() {
    let fixture = Fixture::new();
    let sculpt = SculptEntry {
        id: 1,
        name: "Preserved sculpt".into(),
        settings: SculptBrushSettings {
            tool: pentimento_ipc::SculptTool::Push,
            radius: 0.5,
            strength: 1.,
            hardness: 0.5,
            autosmooth: 0.5,
            falloff: pentimento_ipc::SculptFalloff::Smooth,
        },
        autosmooth_override: None,
    };
    let mut catalog = Catalog::load(Some(fixture.0.clone()));
    let mut document = Document::default();
    document.sculpt.push(sculpt.clone());
    catalog.persist(document).unwrap();
    let mut world = fixture.world();
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "Paint".into(),
        },
    );
    let restored = Catalog::load(Some(fixture.0.clone()));
    assert_eq!(restored.document.sculpt.len(), 1);
    assert_eq!(restored.document.sculpt[0].settings, sculpt.settings);
    assert_eq!(restored.document.paint.len(), 1);
    let mut invalid = fixture.world();
    let missing = fixture
        .0
        .parent()
        .unwrap()
        .join("blocked-parent/brushes.json");
    invalid.insert_resource(Catalog::load(Some(missing.clone())));
    std::fs::write(missing.parent().unwrap(), b"not a directory").unwrap();
    paint(
        &mut invalid,
        PaintCommand::SaveBrushPreset {
            name: "Not saved".into(),
        },
    );
    assert!(invalid.resource::<Catalog>().document.paint.is_empty());
    assert!(
        invalid
            .resource::<Catalog>()
            .notice
            .as_ref()
            .unwrap()
            .contains("Cannot")
    );
    let mut unavailable = fixture.world();
    unavailable.insert_resource(Catalog::load(None));
    paint(
        &mut unavailable,
        PaintCommand::SaveBrushPreset {
            name: "No storage".into(),
        },
    );
    assert!(unavailable.resource::<Catalog>().document.paint.is_empty());
    assert!(matches!(
        message(&unavailable),
        Some(BevyToUi::SavedBrushPresetsChanged {
            available: false,
            ..
        })
    ));
    #[cfg(feature = "sculpting")]
    {
        let mut inconsistent = Document::default();
        let mut invalid_sculpt = sculpt;
        invalid_sculpt.settings.autosmooth = 0.2;
        inconsistent.sculpt.push(invalid_sculpt);
        assert!(inconsistent.validate().is_err());
    }
}

#[cfg(feature = "sculpting")]
#[test]
fn restoring_a_saved_sculpt_brush_changes_guarded_pipeline_geometry_and_keeps_history() {
    let fixture = Fixture::new();
    let mut world = fixture.world();
    for command in [
        SculptCommand::SetRadius { radius: 0.75 },
        SculptCommand::SetStrength { strength: 0.15 },
        SculptCommand::SetAutoSmooth { amount: 0.2 },
    ] {
        sculpt(&mut world, command);
    }
    sculpt(
        &mut world,
        SculptCommand::SaveBrushPreset {
            name: "Gentle push".into(),
        },
    );
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let imported = painting::half_edge::HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let mut mesh = sculpting::partition_mesh(&imported, &sculpting::PartitionConfig::default());
    let before = mesh.clone();
    let mut restarted = fixture.world();
    let pipeline = sculpting::SculptingPipeline::with_config(
        sculpting::BrushPreset {
            radius: 0.75,
            strength: 0.,
            autosmooth: 0.,
            ..sculpting::BrushPreset::push()
        },
        sculpting::PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..default()
        },
    );
    restarted
        .resource_mut::<crate::sculpt_mode::SculptingData>()
        .pipeline = Some(pipeline);
    let input = |x| sculpting::BrushInput {
        position: Vec3::new(x, 0., 1.),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 1,
    };
    {
        let mut data = restarted.resource_mut::<crate::sculpt_mode::SculptingData>();
        let pipeline = data.pipeline.as_mut().unwrap();
        pipeline.begin_stroke(7, input(0.));
        assert!(
            pipeline
                .process_input(input(0.2), &mut mesh)
                .rejected
                .is_none()
        );
        pipeline.end_stroke(&mut mesh);
    }
    for (id, chunk) in &before.chunks {
        for (original, unchanged) in chunk
            .mesh
            .vertices()
            .iter()
            .zip(mesh.chunks[id].mesh.vertices())
        {
            assert_eq!(
                original.position, unchanged.position,
                "Zero strength must not move the surface"
            );
        }
    }
    // Import normals may be canonicalized by the first guarded dab. Establish
    // that validated baseline before comparing the saved brush's actual effect.
    let before = mesh.clone();
    restarted
        .resource_mut::<crate::sculpt_mode::SculptingData>()
        .pipeline
        .as_mut()
        .unwrap()
        .reset_history(7, &mut mesh, None)
        .unwrap();
    sculpt(
        &mut restarted,
        SculptCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    {
        let mut data = restarted.resource_mut::<crate::sculpt_mode::SculptingData>();
        data.pipeline.as_mut().unwrap().begin_stroke(7, input(0.));
    }
    let stored = std::fs::read(&fixture.0).unwrap();
    sculpt(
        &mut restarted,
        SculptCommand::SaveBrushPreset {
            name: "Blocked".into(),
        },
    );
    sculpt(
        &mut restarted,
        SculptCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    assert_eq!(std::fs::read(&fixture.0).unwrap(), stored);
    assert!(
        restarted
            .resource::<crate::sculpt_mode::SculptingData>()
            .pipeline
            .as_ref()
            .unwrap()
            .is_stroke_active()
    );
    let mut data = restarted.resource_mut::<crate::sculpt_mode::SculptingData>();
    let pipeline = data.pipeline.as_mut().unwrap();
    assert!(
        pipeline
            .process_input(input(0.2), &mut mesh)
            .rejected
            .is_none()
    );
    assert!(pipeline.end_stroke(&mut mesh).rejected.is_none());
    let after = mesh.clone();
    assert!(!before.same_authoritative_state(&after));
    assert_eq!(pipeline.history_status().undo_strokes, 1);
    assert!(pipeline.restore_history(&mut mesh, false).unwrap());
    assert!(before.same_authoritative_state(&mesh));
    assert!(pipeline.restore_history(&mut mesh, true).unwrap());
    assert!(after.same_authoritative_state(&mesh));
}

pub(crate) struct Fixture(PathBuf);
impl Fixture {
    pub(crate) fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pentimento-preset-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.join("brushes.json"))
    }
    pub(crate) fn install(&self, world: &mut World) {
        world.insert_resource(Catalog::load(Some(self.0.clone())));
    }
    fn world(&self) -> World {
        let mut world = World::new();
        world.init_resource::<PaintingResource>();
        world.init_resource::<OutboundUiMessages>();
        world.init_resource::<crate::PaintMode>();
        #[cfg(feature = "sculpting")]
        {
            world.init_resource::<crate::SculptState>();
            world.init_resource::<crate::sculpt_mode::SculptingData>();
        }
        world.insert_resource(Catalog::load(Some(self.0.clone())));
        world
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}
fn paint(world: &mut World, command: PaintCommand) {
    assert!(dispatch_brush_ui_command(
        world,
        &UiToBevy::PaintCommand(command)
    ));
}

#[test]
fn simultaneous_editor_saves_do_not_overwrite_each_other() {
    let fixture = Fixture::new();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = vec![];
    for name in ["First editor", "Second editor"] {
        let path = fixture.0.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut catalog = Catalog::load(Some(path));
            let mut document = Document::default();
            document.paint.push(PaintEntry {
                id: 1,
                name: name.into(),
                brush: painting::BrushPreset::default(),
                color: [0., 0., 0., 1.],
                blend: pentimento_ipc::BlendMode::Normal,
            });
            barrier.wait();
            catalog.persist(document).is_ok()
        }));
    }
    assert_eq!(
        handles
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>(),
        1
    );
    let restarted = fixture.world();
    assert!(!restarted.resource::<Catalog>().blocked);
    assert_eq!(restarted.resource::<Catalog>().document.paint.len(), 1);
}
#[cfg(feature = "sculpting")]
fn sculpt(world: &mut World, command: SculptCommand) {
    assert!(dispatch_brush_ui_command(
        world,
        &UiToBevy::SculptCommand(command)
    ));
}

#[test]
fn paint_presets_restart_restore_full_engine_pressure_color_eraser_and_pixels() {
    let fixture = Fixture::new();
    let mut world = fixture.world();
    for command in [
        PaintCommand::SetBrushSize { size: 32. },
        PaintCommand::SetBrushHardness { hardness: 1. },
        PaintCommand::SetBrushOpacity { opacity: 0.7 },
        PaintCommand::SetBrushSpacing { spacing: 0.15 },
        PaintCommand::SetBrushColor {
            color: [1., 0., 0., 1.],
        },
    ] {
        paint(&mut world, command);
    }
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "Round".into(),
        },
    );
    let expected = world.resource::<PaintingResource>().brush_preset.clone();
    assert_eq!(world.resource::<Catalog>().document.paint.len(), 1);
    let mut restarted = fixture.world();
    restarted
        .resource_mut::<PaintingResource>()
        .get_or_create_pipeline(7, 128, 128);
    paint(
        &mut restarted,
        PaintCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    let mut resource = restarted.resource_mut::<PaintingResource>();
    assert_eq!(resource.brush_preset, expected);
    let pipeline = resource.get_pipeline_mut(7).unwrap();
    assert_eq!(
        pipeline.brush_preset().size_for_pressure(0.),
        expected.min_size
    );
    assert_eq!(pipeline.brush_preset().size_for_pressure(1.), 32.);
    assert_eq!(pipeline.color(), [1., 0., 0., 1.]);
    pipeline.begin_stroke(7, 1, 0);
    pipeline.stroke_to(64., 64., 1.);
    pipeline.end_stroke();
    pipeline.layers.composite();
    let painted = pipeline.surface_as_bytes().to_vec();
    assert!(pipeline.get_pixel(74, 64).unwrap()[3] > 0.6);
    assert_eq!(pipeline.undo_count(), 1);
    drop(resource);
    paint(
        &mut restarted,
        PaintCommand::SetBlendMode {
            mode: pentimento_ipc::BlendMode::Erase,
        },
    );
    paint(
        &mut restarted,
        PaintCommand::SaveBrushPreset {
            name: "Erase".into(),
        },
    );
    paint(
        &mut restarted,
        PaintCommand::SetBlendMode {
            mode: pentimento_ipc::BlendMode::Normal,
        },
    );
    paint(
        &mut restarted,
        PaintCommand::SelectSavedBrushPreset { preset_id: 2 },
    );
    assert_eq!(
        restarted.resource::<PaintingResource>().blend_mode,
        painting::BlendMode::Erase
    );
    let mut resource = restarted.resource_mut::<PaintingResource>();
    let pipeline = resource.get_pipeline_mut(7).unwrap();
    assert_eq!((pipeline.undo_count(), pipeline.redo_count()), (1, 0));
    pipeline.begin_stroke(7, 2, 0);
    pipeline.stroke_to(64., 64., 1.);
    pipeline.end_stroke();
    pipeline.layers.composite();
    assert_ne!(pipeline.surface_as_bytes(), painted);
    assert!(pipeline.undo());
    pipeline.layers.composite();
    assert_eq!(pipeline.surface_as_bytes(), painted);
}

#[test]
fn save_and_select_refuse_pipeline_owned_strokes_without_mutation_or_files() {
    let fixture = Fixture::new();
    let mut world = fixture.world();
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "First".into(),
        },
    );
    paint(&mut world, PaintCommand::SetBrushSize { size: 80. });
    let before = world.resource::<PaintingResource>().brush_preset.clone();
    let stored = std::fs::read(&fixture.0).unwrap();
    {
        let mut resource = world.resource_mut::<PaintingResource>();
        let pipeline = resource.get_or_create_pipeline(7, 128, 128);
        pipeline.begin_stroke(7, 1, 0);
        pipeline.stroke_to(64., 64., 1.);
    }
    for command in [
        PaintCommand::SaveBrushPreset {
            name: "Refused".into(),
        },
        PaintCommand::SelectSavedBrushPreset { preset_id: 1 },
        PaintCommand::SelectBrushPreset { preset_id: 2 },
    ] {
        paint(&mut world, command);
    }
    assert_eq!(world.resource::<PaintingResource>().brush_preset, before);
    assert!(
        world
            .resource::<PaintingResource>()
            .get_pipeline(7)
            .unwrap()
            .is_stroking()
    );
    assert_eq!(world.resource::<Catalog>().document.paint.len(), 1);
    assert_eq!(std::fs::read(&fixture.0).unwrap(), stored);
    assert!(
        world
            .resource::<OutboundUiMessages>()
            .messages
            .iter()
            .filter(
                |m| matches!(m, BevyToUi::Error { code, .. } if code == "brush_preset_rejected")
            )
            .count()
            >= 3
    );
}

#[test]
fn replacement_bounds_corrupt_storage_and_external_changes_are_honest() {
    let fixture = Fixture::new();
    let mut world = fixture.world();
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "Round".into(),
        },
    );
    paint(&mut world, PaintCommand::SetBrushSize { size: 90. });
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: " Round ".into(),
        },
    );
    assert_eq!(world.resource::<Catalog>().document.paint.len(), 1);
    assert_eq!(world.resource::<Catalog>().document.paint[0].id, 1);
    assert_eq!(
        fixture.world().resource::<Catalog>().document.paint[0]
            .brush
            .max_size,
        90.
    );
    let original = std::fs::read(&fixture.0).unwrap();
    for name in ["".to_owned(), "a".repeat(65), "bad\nname".to_owned()] {
        paint(&mut world, PaintCommand::SaveBrushPreset { name });
    }
    assert_eq!(std::fs::read(&fixture.0).unwrap(), original);
    std::fs::write(&fixture.0, b"external edit").unwrap();
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "Other".into(),
        },
    );
    assert_eq!(std::fs::read(&fixture.0).unwrap(), b"external edit");
    assert!(world.resource::<Catalog>().blocked);
    let mut invalid = fixture.world();
    assert!(invalid.resource::<Catalog>().blocked);
    paint(
        &mut invalid,
        PaintCommand::SaveBrushPreset {
            name: "Refused".into(),
        },
    );
    assert_eq!(std::fs::read(&fixture.0).unwrap(), b"external edit");
    let mut document = Document::default();
    for id in 1..=65 {
        document.paint.push(PaintEntry {
            id,
            name: format!("Brush {id}"),
            brush: painting::BrushPreset::default(),
            color: [0., 0., 0., 1.],
            blend: pentimento_ipc::BlendMode::Normal,
        });
    }
    assert!(document.validate().is_err());
    document.paint.pop();
    assert!(document.validate().is_ok());
    document.paint[0].brush.spacing = 0.;
    assert!(document.validate().is_err());
    document.paint[0].brush.spacing = 0.25;
    std::fs::write(&fixture.0, serde_json::to_vec(&document).unwrap()).unwrap();
    let mut full = fixture.world();
    let bounded = std::fs::read(&fixture.0).unwrap();
    paint(
        &mut full,
        PaintCommand::SaveBrushPreset {
            name: "Over the limit".into(),
        },
    );
    assert_eq!(std::fs::read(&fixture.0).unwrap(), bounded);
    paint(&mut full, PaintCommand::SetBrushSize { size: 12. });
    paint(
        &mut full,
        PaintCommand::SaveBrushPreset {
            name: "Brush 64".into(),
        },
    );
    assert_eq!(full.resource::<Catalog>().document.paint.len(), 64);
    assert_eq!(full.resource::<Catalog>().document.paint[63].id, 64);
    assert_eq!(
        fixture.world().resource::<Catalog>().document.paint[63]
            .brush
            .max_size,
        12.
    );
}

#[cfg(feature = "sculpting")]
#[test]
fn sculpt_presets_restart_keep_modes_separate_and_preserve_grab_override() {
    let fixture = Fixture::new();
    let mut world = fixture.world();
    for command in [
        SculptCommand::SetRadius { radius: 1.2 },
        SculptCommand::SetStrength { strength: 0.21 },
        SculptCommand::SetHardness { hardness: 0.4 },
        SculptCommand::SetFalloff {
            falloff: pentimento_ipc::SculptFalloff::Sharp,
        },
        SculptCommand::SetAutoSmooth { amount: 0.17 },
        SculptCommand::SetTool {
            tool: pentimento_ipc::SculptTool::Crease,
        },
    ] {
        sculpt(&mut world, command);
    }
    sculpt(
        &mut world,
        SculptCommand::SaveBrushPreset {
            name: "Same name".into(),
        },
    );
    let expected = crate::sculpt_mode::sculpt_snapshot(world.resource::<crate::SculptState>());
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "Same name".into(),
        },
    );
    sculpt(
        &mut world,
        SculptCommand::SetTool {
            tool: pentimento_ipc::SculptTool::Grab,
        },
    );
    sculpt(
        &mut world,
        SculptCommand::SaveBrushPreset {
            name: "Grab".into(),
        },
    );
    let mut restarted = fixture.world();
    restarted
        .resource_mut::<crate::sculpt_mode::SculptingData>()
        .pipeline = Some(sculpting::SculptingPipeline::new(
        sculpting::BrushPreset::push(),
    ));
    let paint_before = restarted
        .resource::<PaintingResource>()
        .brush_preset
        .clone();
    sculpt(
        &mut restarted,
        SculptCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    assert_eq!(
        crate::sculpt_mode::sculpt_snapshot(restarted.resource::<crate::SculptState>()),
        expected
    );
    assert_eq!(
        restarted.resource::<PaintingResource>().brush_preset,
        paint_before
    );
    assert_eq!(
        restarted
            .resource::<crate::sculpt_mode::SculptingData>()
            .pipeline
            .as_ref()
            .unwrap()
            .brush_preset()
            .autosmooth,
        0.17
    );
    sculpt(
        &mut restarted,
        SculptCommand::SelectSavedBrushPreset { preset_id: 2 },
    );
    let preset = restarted
        .resource::<crate::sculpt_mode::SculptingData>()
        .pipeline
        .as_ref()
        .unwrap()
        .brush_preset();
    assert_eq!((preset.spacing, preset.autosmooth), (0., 0.));
    assert_eq!(
        restarted.resource::<crate::SculptState>().brush_autosmooth,
        Some(0.17)
    );
    // A stored Grab override becomes effective again on a stamped brush.
    sculpt(
        &mut restarted,
        SculptCommand::SetTool {
            tool: pentimento_ipc::SculptTool::Push,
        },
    );
    assert_eq!(
        restarted
            .resource::<crate::sculpt_mode::SculptingData>()
            .pipeline
            .as_ref()
            .unwrap()
            .brush_preset()
            .autosmooth,
        0.17
    );
    assert_eq!(
        restarted.resource::<crate::SculptState>().brush_autosmooth,
        Some(0.17)
    );
    restarted
        .resource_mut::<crate::SculptState>()
        .current_stroke_id = Some(77);
    let before = crate::sculpt_mode::sculpt_snapshot(restarted.resource::<crate::SculptState>());
    sculpt(
        &mut restarted,
        SculptCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    sculpt(
        &mut restarted,
        SculptCommand::SaveBrushPreset {
            name: "Refused".into(),
        },
    );
    assert_eq!(
        crate::sculpt_mode::sculpt_snapshot(restarted.resource::<crate::SculptState>()),
        before
    );
    assert_eq!(
        restarted.resource::<crate::SculptState>().current_stroke_id,
        Some(77)
    );
    assert_eq!(restarted.resource::<Catalog>().document.sculpt.len(), 2);
}

#[test]
fn integrated_preset_selection_disarms_only_on_success_and_external_edits_stay_blocked() {
    let fixture = Fixture::new();
    let mut world = fixture.world();
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "Paint".into(),
        },
    );
    world.resource_mut::<crate::PaintMode>().sample_color = true;
    paint(
        &mut world,
        PaintCommand::SelectSavedBrushPreset {
            preset_id: u32::MAX,
        },
    );
    assert!(world.resource::<crate::PaintMode>().sample_color);
    paint(
        &mut world,
        PaintCommand::SelectBrushPreset {
            preset_id: u32::MAX,
        },
    );
    assert!(world.resource::<crate::PaintMode>().sample_color);
    world.resource_mut::<crate::PaintMode>().current_stroke = Some(crate::StrokeState {
        stroke_id: 99,
        space_id: 7,
        start_time: 0,
        last_world_pos: None,
        last_time: 0.,
    });
    paint(
        &mut world,
        PaintCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    paint(&mut world, PaintCommand::SelectBrushPreset { preset_id: 0 });
    assert!(world.resource::<crate::PaintMode>().sample_color);
    world.resource_mut::<crate::PaintMode>().current_stroke = None;
    paint(&mut world, PaintCommand::SelectBrushPreset { preset_id: 0 });
    assert!(!world.resource::<crate::PaintMode>().sample_color);
    world.resource_mut::<crate::PaintMode>().sample_color = true;
    let original = world.resource::<PaintingResource>().brush_preset.clone();
    let mut external: Document =
        serde_json::from_slice(&std::fs::read(&fixture.0).unwrap()).unwrap();
    external.paint[0].brush.max_size = 99.;
    let external_bytes = serde_json::to_vec(&external).unwrap();
    std::fs::write(&fixture.0, &external_bytes).unwrap();
    // Existing policy detects external edits while saving, then blocks recall.
    paint(
        &mut world,
        PaintCommand::SaveBrushPreset {
            name: "Paint".into(),
        },
    );
    assert_eq!(std::fs::read(&fixture.0).unwrap(), external_bytes);
    paint(
        &mut world,
        PaintCommand::SelectSavedBrushPreset { preset_id: 1 },
    );
    assert!(world.resource::<crate::PaintMode>().sample_color);
    assert_eq!(world.resource::<PaintingResource>().brush_preset, original);
    assert!(world.resource::<Catalog>().blocked);
}
