//! Routes the existing paint controls to one authoring backend. No frontend brush state.
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use pentimento_ipc::{BevyToUi, PaintCommand, PaintTarget, PaintTargetState};

pub(crate) struct PaintBrushValue {
    pub brush_preset: painting::BrushPreset,
    pub brush_color: [f32; 4],
    pub blend_mode: painting::BlendMode,
}
impl PaintBrushValue {
    pub fn set_brush_preset(&mut self, preset: painting::BrushPreset) {
        self.brush_preset = preset;
    }
    pub fn set_brush_color(&mut self, color: [f32; 4]) {
        self.brush_color = color;
    }
    pub fn set_blend_mode_ipc(&mut self, mode: pentimento_ipc::BlendMode) {
        self.blend_mode = match mode {
            pentimento_ipc::BlendMode::Normal => painting::BlendMode::Normal,
            pentimento_ipc::BlendMode::Erase => painting::BlendMode::Erase,
        };
    }
}
pub(crate) fn is_direct(world: &World) -> bool {
    world
        .get_resource::<crate::PaintMode>()
        .is_some_and(|p| p.target == PaintTarget::DirectUv)
}
pub(crate) fn brush(world: &World) -> Option<PaintBrushValue> {
    #[cfg(feature = "mesh_painting")]
    if is_direct(world) {
        let r = world.get_resource::<crate::MeshPaintingResource>()?;
        return Some(PaintBrushValue {
            brush_preset: r.brush_preset.clone(),
            brush_color: r.brush_color,
            blend_mode: r.blend_mode,
        });
    }
    let r = world.get_resource::<crate::PaintingResource>()?;
    Some(PaintBrushValue {
        brush_preset: r.brush_preset.clone(),
        brush_color: r.brush_color,
        blend_mode: r.blend_mode,
    })
}
pub(crate) fn set_brush(world: &mut World, brush: PaintBrushValue) -> Result<(), String> {
    #[cfg(feature = "mesh_painting")]
    if is_direct(world) {
        let mut r = world
            .get_resource_mut::<crate::MeshPaintingResource>()
            .ok_or("DirectUV brushes are unavailable")?;
        r.set_brush_preset(brush.brush_preset);
        r.set_brush_color(brush.brush_color);
        r.set_blend_mode(brush.blend_mode);
        return Ok(());
    }
    let mut r = world
        .get_resource_mut::<crate::PaintingResource>()
        .ok_or("Canvas brushes are unavailable")?;
    r.set_brush_preset(brush.brush_preset);
    r.set_brush_color(brush.brush_color);
    r.set_blend_mode(brush.blend_mode);
    Ok(())
}
pub(crate) fn error(world: &mut World, message: impl Into<String>) {
    let message = message.into();
    if let Some(mut p) = world.get_resource_mut::<crate::PaintMode>() {
        p.target_notice = Some(message.clone());
    }
    if let Some(mut out) = world.get_resource_mut::<crate::OutboundUiMessages>() {
        out.send(BevyToUi::Error {
            code: "paint_target_rejected".into(),
            message,
        });
    }
}
#[cfg(feature = "mesh_painting")]
pub(crate) fn admitted(world: &World, entity: Entity) -> bool {
    let Some(p) = world.get::<crate::PaintableMesh>(entity) else {
        return false;
    };
    if !matches!(p.storage_mode, painting::MeshStorageMode::UvAtlas { .. })
        || world.get::<crate::CanvasPlane>(entity).is_some()
    {
        return false;
    }
    let Some(h) = world.get::<Mesh3d>(entity) else {
        return false;
    };
    let valid_mesh = world
        .get_resource::<Assets<Mesh>>()
        .and_then(|m| m.get(&h.0))
        .and_then(|m| crate::project_assets::MeshDocument::capture(m).ok())
        .is_some_and(|m| m.has_uv0() && m.validate().is_ok());
    let Some(h) = world.get::<MeshMaterial3d<StandardMaterial>>(entity) else {
        return false;
    };
    if world
        .get_resource::<crate::MeshPaintingResource>()
        .is_some_and(|r| r.uv_layers(p.mesh_id).is_some())
    {
        return valid_mesh
            && world
                .get_resource::<Assets<StandardMaterial>>()
                .and_then(|m| m.get(&h.0))
                .is_some_and(|m| {
                    crate::uv_layer_scene::validate_display(world, entity, m).is_ok()
                })
            && !world
                .resource::<crate::MeshPaintingResource>()
                .history_conflicted(p.mesh_id);
    }
    valid_mesh
        && world
            .get_resource::<Assets<StandardMaterial>>()
            .and_then(|m| m.get(&h.0))
            .is_some_and(|m| crate::project_uv::capture(world, entity, m).is_ok())
        && !world
            .get_resource::<crate::MeshPaintingResource>()
            .is_some_and(|r| r.history_conflicted(p.mesh_id))
}
#[cfg(feature = "mesh_painting")]
fn candidate(world: &mut World) -> Option<Entity> {
    if let Some(e) = world
        .get_resource::<crate::PaintMode>()
        .and_then(|p| p.direct_target)
        .filter(|e| admitted(world, *e))
    {
        return Some(e);
    }
    #[cfg(feature = "selection")]
    if let Some(e) = world
        .query_filtered::<Entity, With<crate::Selected>>()
        .iter(world)
        .find(|e| admitted(world, *e))
    {
        return Some(e);
    }
    let mut meshes: Vec<_> = world
        .query::<(Entity, &crate::PaintableMesh)>()
        .iter(world)
        .map(|(e, p)| (p.mesh_id, e))
        .collect();
    meshes.sort_by_key(|(id, _)| *id);
    meshes
        .into_iter()
        .map(|(_, e)| e)
        .find(|e| admitted(world, *e))
}
pub(crate) fn set_target(world: &mut World, target: PaintTarget) {
    if crate::brush_presets::active(world)
        || world
            .get_resource::<crate::FrontendScenePointerInput>()
            .is_some_and(|s| s.has_scene_press())
    {
        error(
            world,
            "Finish or cancel the current stroke before changing painting mode.",
        );
        return;
    }
    if !world
        .get_resource::<crate::PaintMode>()
        .is_some_and(|p| p.active)
    {
        error(world, "Enter paint mode before choosing a painting target.");
        return;
    }
    if target == PaintTarget::DirectUv {
        if world
            .get_resource::<Messages<crate::ProjectionEvent>>()
            .is_some_and(|events| !events.is_empty())
        {
            error(
                world,
                "Wait for the pending Canvas projection command before changing painting mode.",
            );
            return;
        }
        #[cfg(feature = "mesh_painting")]
        {
            let Some(entity) = candidate(world) else {
                error(
                    world,
                    "No supported DirectUV receiver is ready. Use a UV-mapped mesh with an owned RGBA8 sRGB appearance; PTex and mixed projection ownership are unsupported.",
                );
                return;
            };
            let canvas = world
                .get_resource::<crate::ActiveCanvasPlane>()
                .and_then(|a| a.entity);
            let visibility = canvas.and_then(|e| world.get::<Visibility>(e).copied());
            let locked = world
                .get_resource::<crate::ActiveCanvasPlane>()
                .map(|a| a.camera_locked);
            if let Some(e) = canvas {
                if let Some(mut v) = world.get_mut::<Visibility>(e) {
                    *v = Visibility::Hidden;
                }
            }
            if let Some(mut a) = world.get_resource_mut::<crate::ActiveCanvasPlane>() {
                a.camera_locked = false;
            }
            if let Some(mut projection) = world.get_resource_mut::<crate::ProjectionMode>() {
                projection.live_projection = false;
            }
            let mut p = world.resource_mut::<crate::PaintMode>();
            if p.target != target {
                p.direct_source_entity = canvas;
                p.direct_source_visibility = visibility;
                p.direct_camera_locked = locked;
            }
            p.target = target;
            p.direct_target = Some(entity);
            p.sample_color = false;
            p.target_notice = None;
            crate::brush_presets::clear_paint_selection(world);
            return;
        }
        #[cfg(not(feature = "mesh_painting"))]
        {
            error(
                world,
                "DirectUV painting is unavailable in this renderer build.",
            );
            return;
        }
    }
    restore_canvas(world);
    crate::brush_presets::clear_paint_selection(world);
}
/// Return saved view properties to their original owner, including ordinary mode exit.
pub(crate) fn restore_canvas(world: &mut World) {
    let (entity, visibility, locked) = {
        let mut p = world.resource_mut::<crate::PaintMode>();
        p.target = PaintTarget::Canvas;
        p.sample_color = false;
        p.target_notice = None;
        (
            p.direct_source_entity.take(),
            p.direct_source_visibility.take(),
            p.direct_camera_locked.take(),
        )
    };
    if let Some(e) = entity {
        if let (Some(v), Some(mut current)) = (visibility, world.get_mut::<Visibility>(e)) {
            *current = v;
        }
    }
    if let Some(mut a) = world.get_resource_mut::<crate::ActiveCanvasPlane>() {
        if a.entity == entity {
            if let Some(locked) = locked {
                a.camera_locked = locked;
            }
        }
    }
}
pub(crate) fn cancel(world: &mut World) {
    if is_direct(world) {
        #[cfg(feature = "mesh_painting")]
        {
            if let Some(mut p) = world.get_resource_mut::<crate::MeshPaintState>() {
                p.current_stroke = None;
                p.active_mesh = None;
            }
            if let Some(mut events) = world.get_resource_mut::<Messages<crate::MeshPaintEvent>>() {
                events.write(crate::MeshPaintEvent::StrokeCancel);
            }
        }
    } else {
        if let Some(mut p) = world.get_resource_mut::<crate::PaintMode>() {
            p.current_stroke = None;
            p.sample_color = false;
        }
        if let Some(mut events) = world.get_resource_mut::<Messages<crate::PaintEvent>>() {
            events.write(crate::PaintEvent::StrokeCancel);
        }
    }
}
/// Returns true for commands handled or refused at this backend boundary.
pub(crate) fn command(world: &mut World, command: &PaintCommand) -> bool {
    #[cfg(feature = "mesh_painting")]
    if crate::uv_layer_scene::command(world, command) {
        return true;
    }
    match command {
        PaintCommand::SetTarget { target } => {
            set_target(world, *target);
            return true;
        }
        PaintCommand::CancelStroke => {
            cancel(world);
            return true;
        }
        _ => {}
    }
    if !is_direct(world) {
        return false;
    }
    if crate::brush_presets::active(world) {
        error(
            world,
            "Finish or cancel the current DirectUV stroke before editing its brush or history.",
        );
        return true;
    }
    match command {
        PaintCommand::Undo | PaintCommand::Redo => {
            #[cfg(feature = "mesh_painting")]
            {
                let entity = world
                    .get_resource::<crate::PaintMode>()
                    .and_then(|p| p.direct_target);
                let restored = entity.is_some_and(|e| {
                    if matches!(command, PaintCommand::Undo) {
                        crate::undo_mesh_paint(world, e)
                    } else {
                        crate::redo_mesh_paint(world, e)
                    }
                });
                if restored {
                    world.resource_mut::<crate::PaintMode>().target_notice = None;
                } else {
                    error(
                        world,
                        "DirectUV history is unavailable or conflicted. Wait for display settlement, or reopen the owned project.",
                    );
                }
            }
            true
        }
        PaintCommand::SetColorSampling { .. }
        | PaintCommand::SetColorSampleSource { .. }
        | PaintCommand::SetSourceVisible { .. }
        | PaintCommand::SetLiveProjection { .. }
        | PaintCommand::ProjectToScene
        | PaintCommand::AddLayer { .. }
        | PaintCommand::RemoveLayer { .. }
        | PaintCommand::SetActiveLayer { .. }
        | PaintCommand::SetLayerVisibility { .. }
        | PaintCommand::SetLayerOpacity { .. }
        | PaintCommand::ReorderLayer { .. }
        | PaintCommand::RenameLayer { .. } => {
            error(
                world,
                "This control belongs to Canvas projection. Choose Canvas projection to use its source layers and sampler.",
            );
            true
        }
        _ => false,
    }
}
pub(crate) fn status(world: &mut World) -> PaintTargetState {
    let mut state = PaintTargetState::default();
    if let Some(p) = world.get_resource::<crate::PaintMode>() {
        state.mode = p.target;
        state.notice = p.target_notice.clone();
    }
    state.active = crate::brush_presets::active(world);
    #[cfg(feature = "mesh_painting")]
    {
        state.uv_layers = Some(crate::uv_layer_scene::state(world));
        state.active |= state
            .uv_layers
            .as_ref()
            .is_some_and(|s| s.active && !s.projection_preview);
        state.direct_available = world
            .query::<(&crate::PaintableMesh, &crate::MeshPaintTexture)>()
            .iter(world)
            .any(|(p, _)| matches!(p.storage_mode, painting::MeshStorageMode::UvAtlas { .. }));
        let entity = world
            .get_resource::<crate::PaintMode>()
            .and_then(|p| p.direct_target);
        state.target_name = entity
            .and_then(|e| world.get::<Name>(e))
            .map(|n| n.as_str().into());
        if let Some(r) = world.get_resource::<crate::MeshPaintingResource>() {
            state.retained_bytes = r.history_bytes();
            state.pending_bytes = r.pending_history_bytes()
                + world
                    .get_resource::<crate::projection_painting::PendingUvApplies>()
                    .map_or(0, |p| p.bytes())
                + world
                    .get_resource::<crate::projection_painting::LiveUvPreview>()
                    .map_or(0, |p| {
                        p.unshared_bytes(
                            world.get_resource::<crate::projection_painting::PendingUvApplies>(),
                        )
                    });
            state.limit_bytes = r.history_limit_bytes();
            state.evicted_strokes = r.evicted_history_strokes();
            if entity
                .and_then(|e| world.get::<crate::PaintableMesh>(e))
                .is_some_and(|p| r.history_conflicted(p.mesh_id))
            {
                state.notice=Some("DirectUV ownership changed. Reopen the owned project before editing this receiver.".into());
            }
        }
    }
    state
}
