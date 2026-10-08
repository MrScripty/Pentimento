//! Shared brush command handling and authoritative UI snapshots.
use crate::{
    ActiveCanvasPlane, CanvasPlane, OutboundUiMessages, PaintingResource, ProjectionEvent,
    ProjectionMode,
};
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use painting::PaintingPipeline;
use pentimento_ipc::{
    BevyToUi, BlendMode, LayerInfo, PaintBrushPresetInfo, PaintBrushSettings, PaintCommand,
    UiToBevy,
};

pub fn dispatch_brush_ui_command(world: &mut World, command: &UiToBevy) -> bool {
    match command {
        UiToBevy::LayoutUpdate(layout) => {
            if let Some(mut state) = world.get_resource_mut::<crate::FrontendUiLayout>() {
                state.regions = layout.regions.clone();
                state.received = true;
            }
        }
        UiToBevy::SetUiInputCapture { keyboard } => {
            if let Some(mut state) = world.get_resource_mut::<crate::FrontendInputBlockState>() {
                state.block_keyboard = *keyboard;
            }
        }
        UiToBevy::RequestBrushState => {
            send_brush_state(world);
            let mut mode = world
                .get_resource::<crate::EditModeState>()
                .map_or(pentimento_ipc::EditMode::None, |s| s.mode);
            if world
                .get_resource::<crate::PaintMode>()
                .is_some_and(|s| s.active)
            {
                mode = pentimento_ipc::EditMode::Paint;
            }
            #[cfg(feature = "sculpting")]
            if world
                .get_resource::<crate::SculptState>()
                .is_some_and(|s| s.active)
            {
                mode = pentimento_ipc::EditMode::Sculpt;
            }
            if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
                outbound.send(BevyToUi::EditModeChanged { mode });
            }
        }
        UiToBevy::PaintCommand(command) => {
            if color_sampling_command(world, command) {
                send_brush_state(world);
                return true;
            }
            if matches!(command, PaintCommand::SetBlendMode { .. }) {
                if let Some(mut mode) = world.get_resource_mut::<crate::PaintMode>() {
                    mode.sample_color = false;
                }
            }
            if crate::brush_presets::paint_command(world, command) {
                send_brush_state(world);
                return true;
            }
            if matches!(command, PaintCommand::SelectBrushPreset { .. })
                && crate::brush_presets::active(world)
            {
                crate::brush_presets::reject(
                    world,
                    "Finish or cancel the active stroke before selecting a brush preset.",
                );
                send_brush_state(world);
                return true;
            }
            match command {
                PaintCommand::SetSourceVisible { visible } => {
                    if let Some(entity) = world
                        .get_resource::<ActiveCanvasPlane>()
                        .and_then(|active| active.entity)
                    {
                        if let Some(mut visibility) = world.get_mut::<Visibility>(entity) {
                            *visibility = if *visible {
                                Visibility::Visible
                            } else {
                                Visibility::Hidden
                            };
                        }
                    }
                }
                PaintCommand::SetLiveProjection { enabled } => {
                    if let Some(mut events) = world.get_resource_mut::<Messages<ProjectionEvent>>()
                    {
                        events.write(ProjectionEvent::SetLiveProjection { enabled: *enabled });
                    }
                }
                PaintCommand::ProjectToScene => {
                    if let Some(mut events) = world.get_resource_mut::<Messages<ProjectionEvent>>()
                    {
                        events.write(ProjectionEvent::ProjectToScene);
                    }
                }
                _ => {
                    let active_plane_id = active_plane_id(world);
                    let mut messages = Vec::new();
                    handle_paint_command(world, active_plane_id, command.clone(), &mut messages);
                    if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
                        for message in messages {
                            outbound.send(message);
                        }
                    }
                }
            }
            send_brush_state(world);
        }
        UiToBevy::SculptCommand(command) => {
            if crate::brush_presets::sculpt_command(world, command) {
                send_brush_state(world);
                return true;
            }
            #[cfg(feature = "sculpting")]
            crate::sculpt_mode::apply_sculpt_command(world, command);
            #[cfg(not(feature = "sculpting"))]
            {
                let _ = command;
            }
            send_brush_state(world);
        }
        _ => return false,
    }
    true
}

fn active_plane_id(world: &World) -> Option<u32> {
    world
        .get_resource::<ActiveCanvasPlane>()
        .and_then(|active| active.entity)
        .and_then(|entity| world.get::<CanvasPlane>(entity))
        .map(|canvas| canvas.plane_id)
}

fn color_sampling_command(world: &mut World, command: &PaintCommand) -> bool {
    if !matches!(
        command,
        PaintCommand::SetColorSampling { .. } | PaintCommand::SetColorSampleSource { .. }
    ) {
        return false;
    }
    let ready = world
        .get_resource::<crate::PaintMode>()
        .is_some_and(|m| m.active)
        && active_plane_id(world).is_some_and(|id| {
            world
                .get_resource::<PaintingResource>()
                .is_some_and(|p| p.get_pipeline(id).is_some())
        });
    if crate::brush_presets::active(world) || !ready {
        if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
            outbound.send(BevyToUi::Error { code: "color_sample_rejected".into(), message: "Enter paint mode and finish or cancel the active stroke before sampling a color.".into() });
        }
    } else if let Some(mut mode) = world.get_resource_mut::<crate::PaintMode>() {
        match command {
            PaintCommand::SetColorSampling { enabled } => mode.sample_color = *enabled,
            PaintCommand::SetColorSampleSource { source } => mode.sample_source = *source,
            _ => unreachable!(),
        }
    }
    true
}

fn sampling_message(world: &World) -> BevyToUi {
    let mode = world.get_resource::<crate::PaintMode>();
    BevyToUi::PaintColorSamplingChanged {
        enabled: mode.is_some_and(|m| m.sample_color),
        source: mode.map_or(pentimento_ipc::ColorSampleSource::default(), |m| {
            m.sample_source
        }),
        active: crate::brush_presets::active(world),
    }
}

pub(crate) fn paint_snapshot(painting: &PaintingResource) -> PaintBrushSettings {
    let preset = &painting.brush_preset;
    PaintBrushSettings {
        preset_id: preset.id,
        customized: !painting::builtin_presets().iter().any(|p| {
            p.id == preset.id
                && p.max_size == preset.max_size
                && p.min_size == preset.min_size
                && p.opacity == preset.opacity
                && p.hardness == preset.hardness
                && p.spacing == preset.spacing
        }),
        color: painting.brush_color,
        size: preset.max_size,
        opacity: preset.opacity,
        hardness: preset.hardness,
        spacing: preset.spacing,
        blend_mode: match painting.blend_mode {
            painting::BlendMode::Normal => BlendMode::Normal,
            painting::BlendMode::Erase => BlendMode::Erase,
        },
    }
}

fn paint_message(world: &World) -> Option<BevyToUi> {
    let painting = world.get_resource::<PaintingResource>()?;
    let can_undo = active_plane_id(world)
        .and_then(|id| painting.get_pipeline(id))
        .is_some_and(|p| p.can_undo());
    Some(BevyToUi::PaintBrushStateChanged {
        settings: paint_snapshot(painting),
        presets: painting::builtin_presets()
            .into_iter()
            .map(|preset| PaintBrushPresetInfo {
                id: preset.id,
                name: preset.name,
            })
            .collect(),
        can_undo,
        source_visible: world
            .get_resource::<ActiveCanvasPlane>()
            .and_then(|a| a.entity)
            .and_then(|e| world.get::<Visibility>(e))
            .is_none_or(|v| *v != Visibility::Hidden),
        can_redo: active_plane_id(world)
            .and_then(|id| painting.get_pipeline(id))
            .is_some_and(|p| p.can_redo()),
    })
}

fn sculpt_history_message(world: &World) -> BevyToUi {
    #[cfg(feature = "sculpting")]
    if world
        .get_resource::<crate::SculptState>()
        .is_some_and(|s| s.active)
    {
        if let Some(pipeline) = world
            .get_resource::<crate::sculpt_mode::SculptingData>()
            .and_then(|d| d.pipeline.as_ref())
        {
            let status = pipeline.history_status();
            return BevyToUi::SculptHistoryChanged {
                undo_strokes: status.undo_strokes,
                redo_strokes: status.redo_strokes,
                active: pipeline.is_stroke_active(),
                notice: pipeline.history_notice().map(str::to_owned),
            };
        }
    }
    let _ = world;
    BevyToUi::SculptHistoryChanged {
        undo_strokes: 0,
        redo_strokes: 0,
        active: false,
        notice: None,
    }
}

fn send_brush_state(world: &mut World) {
    crate::brush_presets::ensure(world);
    let saved = crate::brush_presets::message(world);
    let paint = paint_message(world);
    let live_projection = world
        .get_resource::<ProjectionMode>()
        .is_some_and(|p| p.live_projection);
    #[cfg(feature = "sculpting")]
    let sculpt = world
        .get_resource::<crate::SculptState>()
        .map(crate::sculpt_mode::sculpt_snapshot);
    #[cfg(not(feature = "sculpting"))]
    let sculpt = None;
    let history = sculpt_history_message(world);
    let sampling = sampling_message(world);
    if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
        if let Some(saved) = saved {
            outbound.send(saved);
        }
        outbound.send(history);
        outbound.send(sampling);
        if let Some(paint) = paint {
            outbound.send(paint);
        }
        outbound.send(BevyToUi::ProjectionModeChanged { live_projection });
        outbound.send(BevyToUi::SculptBrushStateChanged { settings: sculpt });
    }
}

/// Publish only changed values, including undo availability after strokes and hotkey adjustments.
pub(crate) fn sync_brush_ui_state(world: &mut World, mut previous: Local<String>) {
    crate::brush_presets::ensure(world);
    let paint = paint_message(world);
    let projection = world
        .get_resource::<ProjectionMode>()
        .is_some_and(|p| p.live_projection);
    #[cfg(feature = "sculpting")]
    let sculpt = world
        .get_resource::<crate::SculptState>()
        .map(crate::sculpt_mode::sculpt_snapshot);
    #[cfg(not(feature = "sculpting"))]
    let sculpt: Option<pentimento_ipc::SculptBrushSettings> = None;
    // Tiny settings payload; serialization is stable and avoids keeping duplicate state resources.
    let key = format!(
        "{:?}|{:?}|{}|{:?}|{:?}|{:?}",
        paint,
        sculpt,
        projection,
        sculpt_history_message(world),
        crate::brush_presets::message(world),
        sampling_message(world)
    );
    if *previous != key {
        *previous = key;
        send_brush_state(world);
    }
}

fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn handle_paint_command(
    world: &mut World,
    active_plane_id: Option<u32>,
    command: PaintCommand,
    outbound_layer_messages: &mut Vec<BevyToUi>,
) {
    let Some(mut painting) = world.get_resource_mut::<PaintingResource>() else {
        return;
    };

    match command {
        PaintCommand::SetColorSampling { .. } | PaintCommand::SetColorSampleSource { .. } => {}
        PaintCommand::SaveBrushPreset { .. } | PaintCommand::SelectSavedBrushPreset { .. } => {}
        PaintCommand::SelectBrushPreset { preset_id } => {
            let presets = painting::brush::builtin_presets();
            if let Some(preset) = presets.into_iter().find(|preset| preset.id == preset_id) {
                painting.set_brush_preset(preset);
            }
        }
        PaintCommand::SetBrushColor { color } => {
            let color =
                std::array::from_fn(|i| finite_clamp(color[i], 0.0, 1.0, painting.brush_color[i]));
            painting.set_brush_color(color);
        }
        PaintCommand::SetBrushSize { size } => {
            let size = finite_clamp(size, 1.0, 512.0, painting.brush_preset.max_size);
            let ratio = painting.brush_preset.min_size / painting.brush_preset.max_size.max(0.01);
            painting.brush_preset.base_size = size;
            painting.brush_preset.min_size = size * ratio.clamp(0.0, 1.0);
            painting.brush_preset.max_size = size;
            let preset = painting.brush_preset.clone();
            painting.set_brush_preset(preset);
        }
        PaintCommand::SetBrushOpacity { opacity } => {
            painting.brush_preset.opacity =
                finite_clamp(opacity, 0.0, 1.0, painting.brush_preset.opacity);
            let preset = painting.brush_preset.clone();
            painting.set_brush_preset(preset);
        }
        PaintCommand::SetBrushHardness { hardness } => {
            painting.brush_preset.hardness =
                finite_clamp(hardness, 0.0, 1.0, painting.brush_preset.hardness);
            let preset = painting.brush_preset.clone();
            painting.set_brush_preset(preset);
        }
        PaintCommand::SetBrushSpacing { spacing } => {
            painting.brush_preset.spacing =
                finite_clamp(spacing, 0.01, 1.0, painting.brush_preset.spacing);
            let preset = painting.brush_preset.clone();
            painting.set_brush_preset(preset);
        }
        PaintCommand::SetBlendMode { mode } => {
            painting.set_blend_mode_ipc(mode);
        }
        PaintCommand::Undo => {
            if let Some(id) = active_plane_id {
                let _ = painting.undo(id);
            }
        }
        PaintCommand::SetSourceVisible { .. } => {}
        PaintCommand::Redo => {
            if let Some(id) = active_plane_id {
                let _ = painting.redo(id);
            }
        }
        PaintCommand::SetLiveProjection { enabled } => {
            debug!("Live projection toggle requested: {}", enabled);
        }
        PaintCommand::ProjectToScene => {
            debug!("Project-to-scene requested");
        }
        PaintCommand::AddLayer { name } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                let _ = pipeline.layers.add_layer(name);
                outbound_layer_messages.push(make_layer_state_message(pipeline));
            });
        }
        PaintCommand::RemoveLayer { layer_id } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                if pipeline.layers.remove_layer(layer_id) {
                    outbound_layer_messages.push(make_layer_state_message(pipeline));
                }
            });
        }
        PaintCommand::SetActiveLayer { layer_id } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                if pipeline.layers.set_active(layer_id) {
                    outbound_layer_messages.push(make_layer_state_message(pipeline));
                }
            });
        }
        PaintCommand::SetLayerVisibility { layer_id, visible } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                pipeline.layers.set_visibility(layer_id, visible);
                outbound_layer_messages.push(make_layer_state_message(pipeline));
            });
        }
        PaintCommand::SetLayerOpacity { layer_id, opacity } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                pipeline.layers.set_opacity(layer_id, opacity);
                outbound_layer_messages.push(make_layer_state_message(pipeline));
            });
        }
        PaintCommand::ReorderLayer {
            layer_id,
            new_index,
        } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                pipeline.layers.reorder(layer_id, new_index as usize);
                outbound_layer_messages.push(make_layer_state_message(pipeline));
            });
        }
        PaintCommand::RenameLayer { layer_id, name } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                pipeline.layers.rename(layer_id, name);
                outbound_layer_messages.push(make_layer_state_message(pipeline));
            });
        }
    }
}

fn with_active_pipeline(
    active_plane_id: Option<u32>,
    painting: &mut PaintingResource,
    apply: impl FnOnce(&mut PaintingPipeline),
) {
    let Some(plane_id) = active_plane_id else {
        return;
    };

    let Some(pipeline) = painting.get_pipeline_mut(plane_id) else {
        return;
    };

    apply(pipeline);
}

fn make_layer_state_message(pipeline: &PaintingPipeline) -> BevyToUi {
    let layers = pipeline
        .layers
        .layer_info()
        .into_iter()
        .map(|layer| LayerInfo {
            id: layer.id,
            name: layer.name,
            visible: layer.visible,
            opacity: layer.opacity,
            is_active: layer.is_active,
        })
        .collect();

    BevyToUi::LayerStateChanged { layers }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_canvas() -> World {
        let mut world = World::new();
        world.init_resource::<PaintingResource>();
        world.init_resource::<OutboundUiMessages>();
        world.init_resource::<ProjectionMode>();
        world.init_resource::<Messages<ProjectionEvent>>();
        let entity = world.spawn(CanvasPlane::new(7, 128, 128, 1.0, 1.0)).id();
        world.insert_resource(ActiveCanvasPlane {
            entity: Some(entity),
            ..default()
        });
        world
            .resource_mut::<PaintingResource>()
            .get_or_create_pipeline(7, 128, 128);
        world
    }

    fn paint(world: &mut World, command: PaintCommand) {
        assert!(dispatch_brush_ui_command(
            world,
            &UiToBevy::PaintCommand(command)
        ));
    }

    fn stamp(world: &mut World) {
        let mut painting = world.resource_mut::<PaintingResource>();
        let pipeline = painting.get_pipeline_mut(7).unwrap();
        pipeline.begin_stroke(7, 1, 0);
        pipeline.stroke_to(64.0, 64.0, 1.0);
        pipeline.end_stroke();
        pipeline.layers.composite();
    }

    fn pixel(world: &World, x: u32, y: u32) -> [f32; 4] {
        world
            .resource::<PaintingResource>()
            .get_pipeline(7)
            .unwrap()
            .get_pixel(x, y)
            .unwrap()
    }

    #[test]
    fn source_visibility_command_preserves_live_projection_pixels_and_history() {
        let mut world = world_with_canvas();
        let entity = world.resource::<ActiveCanvasPlane>().entity.unwrap();
        world.entity_mut(entity).insert(Visibility::Visible);
        world.resource_mut::<ProjectionMode>().live_projection = true;
        stamp(&mut world);
        let pixel_before = pixel(&world, 64, 64);
        paint(
            &mut world,
            PaintCommand::SetSourceVisible { visible: false },
        );
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert!(world.resource::<ProjectionMode>().live_projection);
        assert_eq!(pixel(&world, 64, 64), pixel_before);
        assert_eq!(
            world
                .resource::<PaintingResource>()
                .get_pipeline(7)
                .unwrap()
                .undo_count(),
            1
        );
        assert!(matches!(
            paint_message(&world),
            Some(BevyToUi::PaintBrushStateChanged {
                source_visible: false,
                can_undo: true,
                ..
            })
        ));
        paint(&mut world, PaintCommand::SetSourceVisible { visible: true });
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );
        assert_eq!(pixel(&world, 64, 64), pixel_before);
    }

    #[test]
    fn redo_command_restores_active_canvas_and_reports_real_availability() {
        let mut world = world_with_canvas();
        stamp(&mut world);
        let painted = pixel(&world, 64, 64);
        paint(&mut world, PaintCommand::Undo);
        world
            .resource_mut::<PaintingResource>()
            .get_pipeline_mut(7)
            .unwrap()
            .layers
            .composite();
        assert_eq!(pixel(&world, 64, 64)[3], 0.);
        assert!(matches!(
            paint_message(&world),
            Some(BevyToUi::PaintBrushStateChanged { can_redo: true, .. })
        ));
        paint(&mut world, PaintCommand::Redo);
        world
            .resource_mut::<PaintingResource>()
            .get_pipeline_mut(7)
            .unwrap()
            .layers
            .composite();
        assert_eq!(pixel(&world, 64, 64), painted);
        assert!(matches!(
            paint_message(&world),
            Some(BevyToUi::PaintBrushStateChanged {
                can_redo: false,
                can_undo: true,
                ..
            })
        ));
    }

    #[test]
    fn stale_history_ui_commands_cannot_restore_during_active_paint_transaction() {
        fn surface(world: &mut World) -> Vec<u8> {
            let mut painting = world.resource_mut::<PaintingResource>();
            let pipeline = painting.get_pipeline_mut(7).unwrap();
            // Match the production presentation step after a history command.
            pipeline.layers.composite();
            pipeline.surface_as_bytes().to_vec()
        }
        let mut world = world_with_canvas();
        stamp(&mut world);
        let committed = surface(&mut world);
        paint(&mut world, PaintCommand::Undo);
        let baseline = surface(&mut world);
        assert!(
            committed != baseline,
            "the committed stroke must change actual pixels"
        );
        {
            let mut painting = world.resource_mut::<PaintingResource>();
            let pipeline = painting.get_pipeline_mut(7).unwrap();
            pipeline.begin_stroke(7, 2, 0);
            pipeline.stroke_to(32., 32., 1.);
            pipeline.layers.composite();
        }
        let active = surface(&mut world);
        assert!(
            active != baseline,
            "the active stroke must change actual pixels"
        );
        // A menu can still hold an older enabled receipt while native work starts.
        // Dispatch those actual protocol commands; backend ownership must win.
        for command in [PaintCommand::Undo, PaintCommand::Redo] {
            paint(&mut world, command);
            assert!(
                surface(&mut world) == active,
                "history must not overwrite an active stroke"
            );
            let pipeline = world
                .resource::<PaintingResource>()
                .get_pipeline(7)
                .unwrap();
            assert!(pipeline.is_stroking());
            assert_eq!((pipeline.undo_count(), pipeline.redo_count()), (0, 1));
            assert_eq!(pipeline.log().total_packet_count(), 1);
            assert!(matches!(
                paint_message(&world),
                Some(BevyToUi::PaintBrushStateChanged {
                    can_undo: false,
                    can_redo: false,
                    ..
                })
            ));
        }
        {
            let mut painting = world.resource_mut::<PaintingResource>();
            let pipeline = painting.get_pipeline_mut(7).unwrap();
            pipeline.cancel_stroke();
            pipeline.layers.composite();
        }
        assert!(
            surface(&mut world) == baseline,
            "cancel must restore the complete baseline"
        );
        paint(&mut world, PaintCommand::Redo);
        assert!(
            surface(&mut world) == committed,
            "Redo must restore the complete committed surface"
        );
        let pipeline = world
            .resource::<PaintingResource>()
            .get_pipeline(7)
            .unwrap();
        assert_eq!(
            pipeline.log().total_packet_count(),
            1,
            "restoration must not replay input"
        );
    }

    #[test]
    fn radius_controls_actual_dab_coverage_and_survives_new_canvas() {
        let mut world = world_with_canvas();
        paint(&mut world, PaintCommand::SetBrushHardness { hardness: 1.0 });
        paint(&mut world, PaintCommand::SetBrushSize { size: 8.0 });
        stamp(&mut world);
        assert_eq!(pixel(&world, 74, 64)[3], 0.0);
        paint(&mut world, PaintCommand::Undo);
        paint(&mut world, PaintCommand::SetBrushSize { size: 32.0 });
        stamp(&mut world);
        assert!(pixel(&world, 74, 64)[3] > 0.9);
        let mut resource = world.resource_mut::<PaintingResource>();
        let next = resource.get_or_create_pipeline(8, 128, 128);
        assert_eq!(next.brush_preset().size_for_pressure(1.0), 32.0);
    }

    #[test]
    fn color_opacity_hardness_and_erase_reach_real_surface() {
        let mut world = world_with_canvas();
        paint(&mut world, PaintCommand::SetBrushSize { size: 40.0 });
        paint(
            &mut world,
            PaintCommand::SetBrushColor {
                color: [1.0, 0.0, 0.0, 1.0],
            },
        );
        paint(&mut world, PaintCommand::SetBrushOpacity { opacity: 0.5 });
        paint(&mut world, PaintCommand::SetBrushHardness { hardness: 1.0 });
        stamp(&mut world);
        let center = pixel(&world, 64, 64);
        assert!(center[0] > 0.0 && center[1] == 0.0 && center[3] > 0.4 && center[3] < 0.6);
        let hard_edge = pixel(&world, 80, 64)[3];
        paint(&mut world, PaintCommand::Undo);
        paint(&mut world, PaintCommand::SetBrushHardness { hardness: 0.0 });
        stamp(&mut world);
        assert!(pixel(&world, 80, 64)[3] < hard_edge);
        paint(
            &mut world,
            PaintCommand::SetBlendMode {
                mode: BlendMode::Erase,
            },
        );
        paint(&mut world, PaintCommand::SetBrushOpacity { opacity: 1.0 });
        paint(&mut world, PaintCommand::SetBrushHardness { hardness: 1.0 });
        stamp(&mut world);
        assert!(pixel(&world, 64, 64)[3] < 0.01);
    }

    #[test]
    fn spacing_changes_real_dab_count() {
        let mut world = world_with_canvas();
        paint(&mut world, PaintCommand::SetBrushSize { size: 20.0 });
        paint(&mut world, PaintCommand::SetBrushSpacing { spacing: 0.1 });
        let preset = world.resource::<PaintingResource>().brush_preset.clone();
        let mut dense = painting::BrushEngine::new(preset);
        dense.begin_stroke();
        dense.stroke_to(0.0, 0.0, 1.0);
        let dense_count = dense.stroke_to(100.0, 0.0, 1.0).len();
        paint(&mut world, PaintCommand::SetBrushSpacing { spacing: 1.0 });
        let mut sparse =
            painting::BrushEngine::new(world.resource::<PaintingResource>().brush_preset.clone());
        sparse.begin_stroke();
        sparse.stroke_to(0.0, 0.0, 1.0);
        assert!(dense_count > sparse.stroke_to(100.0, 0.0, 1.0).len() * 5);
    }

    #[test]
    fn undo_uses_active_canvas_and_projection_commands_are_events() {
        let mut world = world_with_canvas();
        stamp(&mut world);
        paint(&mut world, PaintCommand::ProjectToScene);
        paint(
            &mut world,
            PaintCommand::SetLiveProjection { enabled: true },
        );
        let events: Vec<_> = world
            .resource_mut::<Messages<ProjectionEvent>>()
            .drain()
            .collect();
        assert!(matches!(events[0], ProjectionEvent::ProjectToScene));
        assert!(matches!(
            events[1],
            ProjectionEvent::SetLiveProjection { enabled: true }
        ));
        paint(&mut world, PaintCommand::Undo);
        world
            .resource_mut::<PaintingResource>()
            .get_pipeline_mut(7)
            .unwrap()
            .layers
            .composite();
        assert_eq!(pixel(&world, 64, 64)[3], 0.0);
    }

    #[test]
    fn invalid_numeric_settings_are_bounded() {
        let mut world = world_with_canvas();
        paint(&mut world, PaintCommand::SetBrushSize { size: f32::NAN });
        paint(&mut world, PaintCommand::SetBrushOpacity { opacity: 20.0 });
        paint(&mut world, PaintCommand::SetBrushSpacing { spacing: 0.0 });
        let preset = &world.resource::<PaintingResource>().brush_preset;
        assert!(preset.max_size.is_finite());
        assert_eq!(preset.opacity, 1.0);
        assert_eq!(preset.spacing, 0.01);
    }
}
