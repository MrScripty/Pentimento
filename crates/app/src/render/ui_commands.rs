use bevy::ecs::message::Messages;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use painting::PaintingPipeline;
use pentimento_ipc::{
    BevyToUi, CameraCommand, GizmoCommand, LayerInfo, MaterialCommand, ObjectCommand, PaintCommand,
    UiToBevy,
};
use pentimento_scene::{
    ActiveCanvasPlane, AddObjectEvent, CanvasPlane, CanvasPlaneEvent, DepthViewSettings,
    GizmoState, MainCamera, MeshEditEvent, OrbitCamera, OutboundUiMessages, PaintingResource,
    SceneAmbientOcclusion, SceneLighting,
};
#[cfg(feature = "selection")]
use pentimento_scene::{Selectable, Selected, SelectionState};

pub(crate) fn dispatch_ui_commands(
    world: &mut World,
    commands: impl IntoIterator<Item = UiToBevy>,
) {
    let active_plane_id = world
        .get_resource::<ActiveCanvasPlane>()
        .and_then(|active| active.entity)
        .and_then(|entity| world.get::<CanvasPlane>(entity))
        .map(|canvas| canvas.plane_id);

    let mut canvas_events = Vec::new();
    let mut outbound_layer_messages = Vec::new();

    for command in commands {
        match command {
            UiToBevy::CameraCommand(command) => handle_camera_command(world, command),
            UiToBevy::ObjectCommand(command) => handle_object_command(world, command),
            UiToBevy::MaterialCommand(command) => handle_material_command(world, command),
            UiToBevy::AddObject(request) => {
                if let Some(mut events) = world.get_resource_mut::<Messages<AddObjectEvent>>() {
                    events.write(AddObjectEvent(request));
                }
            }
            UiToBevy::AddPaintCanvas(request) => {
                canvas_events.push(CanvasPlaneEvent::CreateInFrontOfCamera {
                    width: request.width.unwrap_or(1024),
                    height: request.height.unwrap_or(1024),
                });
            }
            UiToBevy::UpdateLighting(settings) => {
                if let Some(mut lighting) = world.get_resource_mut::<SceneLighting>() {
                    lighting.settings = settings;
                }
            }
            UiToBevy::UpdateAmbientOcclusion(settings) => {
                if let Some(mut ambient_occlusion) =
                    world.get_resource_mut::<SceneAmbientOcclusion>()
                {
                    ambient_occlusion.update(settings);
                }
            }
            UiToBevy::SetDepthView { enabled } => {
                if let Some(mut settings) = world.get_resource_mut::<DepthViewSettings>() {
                    settings.enabled = enabled;
                }
            }
            UiToBevy::PaintCommand(command) => {
                handle_paint_command(
                    world,
                    active_plane_id,
                    command,
                    &mut outbound_layer_messages,
                );
            }
            UiToBevy::MeshEditCommand(command) => handle_mesh_edit_command(world, command),
            UiToBevy::GizmoCommand(command) => handle_gizmo_command(world, command),
            UiToBevy::UiDirty
            | UiToBevy::LayoutUpdate(_)
            | UiToBevy::StartDiffusion(_)
            | UiToBevy::CancelDiffusion { .. }
            | UiToBevy::UpdateSettings(_)
            | UiToBevy::NodeGraphUpdate(_) => {
                debug!("Unhandled shared UI command: {:?}", command);
            }
        }
    }

    if !canvas_events.is_empty() {
        if let Some(mut messages) = world.get_resource_mut::<Messages<CanvasPlaneEvent>>() {
            for event in canvas_events {
                messages.write(event);
            }
        }
    }

    if !outbound_layer_messages.is_empty() {
        if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
            for message in outbound_layer_messages {
                outbound.send(message);
            }
        }
    }
}

fn handle_camera_command(world: &mut World, command: CameraCommand) {
    let mut query = world.query_filtered::<(&mut OrbitCamera, &Transform), With<MainCamera>>();
    let Some((mut orbit, transform)) = query.iter_mut(world).next() else {
        return;
    };

    match command {
        CameraCommand::Orbit { delta_x, delta_y } => {
            orbit.yaw -= delta_x * orbit.orbit_sensitivity;
            orbit.pitch = (orbit.pitch - delta_y * orbit.orbit_sensitivity).clamp(-1.5, 1.5);
        }
        CameraCommand::Pan { delta_x, delta_y } => {
            let right = transform.rotation * Vec3::X;
            let up = transform.rotation * Vec3::Y;
            let pan_scale = orbit.pan_sensitivity * orbit.distance;
            orbit.target += (-right * delta_x + up * delta_y) * pan_scale;
        }
        CameraCommand::Zoom { delta } => {
            let zoom_amount = delta * orbit.zoom_sensitivity * (orbit.distance * 0.1);
            orbit.distance =
                (orbit.distance - zoom_amount).clamp(orbit.min_distance, orbit.max_distance);
        }
        CameraCommand::SetPosition { position } => {
            let position = Vec3::from(position);
            let offset = position - orbit.target;
            orbit.distance = offset
                .length()
                .clamp(orbit.min_distance, orbit.max_distance);
            if orbit.distance > 0.0 {
                orbit.yaw = offset.x.atan2(offset.z);
                orbit.pitch = (offset.y / orbit.distance).asin().clamp(-1.5, 1.5);
            }
        }
        CameraCommand::SetTarget { target } => {
            orbit.target = Vec3::from(target);
        }
        CameraCommand::Reset => orbit.reset(),
    }
}

fn handle_material_command(world: &mut World, command: MaterialCommand) {
    #[cfg(not(feature = "selection"))]
    {
        let _ = (world, command);
        debug!("Material updates require the selection feature");
        return;
    }

    #[cfg(feature = "selection")]
    let MaterialCommand::UpdateProperty {
        material_id,
        property,
        value,
    } = command
    else {
        debug!("Unhandled material command");
        return;
    };

    #[cfg(feature = "selection")]
    let material_handle = {
        let mut query = world.query::<(&Selectable, &MeshMaterial3d<StandardMaterial>)>();
        query
            .iter(world)
            .find(|(selectable, _)| selectable.id == material_id)
            .map(|(_, handle)| handle.0.clone())
    };

    #[cfg(feature = "selection")]
    let Some(material_handle) = material_handle else {
        return;
    };

    #[cfg(feature = "selection")]
    let Some(mut materials) = world.get_resource_mut::<Assets<StandardMaterial>>() else {
        return;
    };
    #[cfg(feature = "selection")]
    let Some(material) = materials.get_mut(&material_handle) else {
        return;
    };

    #[cfg(feature = "selection")]
    match property.as_str() {
        "metallic" => {
            if let Some(value) = value.as_f64() {
                material.metallic = value as f32;
            }
        }
        "roughness" => {
            if let Some(value) = value.as_f64() {
                material.perceptual_roughness = value as f32;
            }
        }
        _ => debug!("Unhandled material property update: {}", property),
    }
}

fn handle_object_command(world: &mut World, command: ObjectCommand) {
    #[cfg(not(feature = "selection"))]
    {
        let _ = (world, command);
        debug!("Object commands require the selection feature");
        return;
    }

    #[cfg(feature = "selection")]
    {
        let selectable_entities = collect_selectable_entities(world);

        match command {
            ObjectCommand::Select { ids } => {
                let selection_ids: Vec<String> = selectable_entities
                    .iter()
                    .filter(|(_, selectable)| ids.contains(&selectable.id))
                    .map(|(_, selectable)| selectable.id.clone())
                    .collect();

                clear_selection(world, &selectable_entities);
                for (entity, selectable) in &selectable_entities {
                    if ids.contains(&selectable.id) {
                        world.entity_mut(*entity).insert(Selected);
                    }
                }
                update_selection_state(world, selection_ids);
            }
            ObjectCommand::Deselect { ids } => {
                for (entity, selectable) in &selectable_entities {
                    if ids.contains(&selectable.id) {
                        world.entity_mut(*entity).remove::<Selected>();
                    }
                }

                let remaining_ids = selectable_entities
                    .iter()
                    .filter(|(entity, selectable)| {
                        world.get::<Selected>(*entity).is_some() && !ids.contains(&selectable.id)
                    })
                    .map(|(_, selectable)| selectable.id.clone())
                    .collect();
                update_selection_state(world, remaining_ids);
            }
            ObjectCommand::Delete { ids } => {
                for (entity, selectable) in selectable_entities {
                    if ids.contains(&selectable.id) {
                        world.entity_mut(entity).despawn();
                    }
                }
                update_selection_state(world, Vec::new());
            }
            ObjectCommand::Transform { id, transform } => {
                if let Some((entity, _)) = selectable_entities
                    .into_iter()
                    .find(|(_, selectable)| selectable.id == id)
                {
                    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                        if let Some(mut object_transform) = entity_mut.get_mut::<Transform>() {
                            object_transform.translation = Vec3::from(transform.position);
                            object_transform.rotation = Quat::from_xyzw(
                                transform.rotation[0],
                                transform.rotation[1],
                                transform.rotation[2],
                                transform.rotation[3],
                            );
                            object_transform.scale = Vec3::from(transform.scale);
                        }
                    }
                }
            }
            ObjectCommand::SetVisibility { id, visible } => {
                if let Some((entity, _)) = selectable_entities
                    .into_iter()
                    .find(|(_, selectable)| selectable.id == id)
                {
                    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                        if let Some(mut object_visibility) = entity_mut.get_mut::<Visibility>() {
                            *object_visibility = if visible {
                                Visibility::Visible
                            } else {
                                Visibility::Hidden
                            };
                        }
                    }
                }
            }
            ObjectCommand::Rename { id, name } => {
                if let Some((entity, _)) = selectable_entities
                    .into_iter()
                    .find(|(_, selectable)| selectable.id == id)
                {
                    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                        if let Some(mut object_name) = entity_mut.get_mut::<Name>() {
                            *object_name = Name::new(name);
                        }
                    }
                }
            }
            ObjectCommand::Duplicate { ids } => {
                debug!(
                    "Object duplication is not implemented yet for ids: {:?}",
                    ids
                );
            }
        }
    }
}

#[cfg(feature = "selection")]
fn collect_selectable_entities(world: &mut World) -> Vec<(Entity, pentimento_scene::Selectable)> {
    let mut query = world.query::<(Entity, &Selectable)>();
    query
        .iter(world)
        .map(|(entity, selectable)| {
            (
                entity,
                Selectable {
                    id: selectable.id.clone(),
                },
            )
        })
        .collect()
}

#[cfg(feature = "selection")]
fn clear_selection(
    world: &mut World,
    selectable_entities: &[(Entity, pentimento_scene::Selectable)],
) {
    for (entity, _) in selectable_entities {
        world.entity_mut(*entity).remove::<Selected>();
    }
}

#[cfg(feature = "selection")]
fn update_selection_state(world: &mut World, selected_ids: Vec<String>) {
    if let Some(mut selection_state) = world.get_resource_mut::<SelectionState>() {
        selection_state.selected_ids = selected_ids;
    }
}

fn handle_mesh_edit_command(world: &mut World, command: pentimento_ipc::MeshEditCommand) {
    let Some(mut events) = world.get_resource_mut::<Messages<MeshEditEvent>>() else {
        return;
    };

    match command {
        pentimento_ipc::MeshEditCommand::SetSelectionMode(mode) => {
            events.write(MeshEditEvent::SetSelectionMode(mode));
        }
        pentimento_ipc::MeshEditCommand::SetTool(tool) => {
            events.write(MeshEditEvent::SetTool(tool));
        }
        pentimento_ipc::MeshEditCommand::SelectAll => {
            events.write(MeshEditEvent::SelectAll);
        }
        pentimento_ipc::MeshEditCommand::DeselectAll => {
            events.write(MeshEditEvent::DeselectAll);
        }
        pentimento_ipc::MeshEditCommand::InvertSelection => {
            debug!("Mesh edit invert-selection command is not implemented yet");
        }
    }
}

fn handle_gizmo_command(world: &mut World, command: GizmoCommand) {
    let Some(mut gizmo_state) = world.get_resource_mut::<GizmoState>() else {
        return;
    };

    match command {
        GizmoCommand::SetMode(mode) => {
            gizmo_state.mode = mode;
            gizmo_state.axis_constraint = pentimento_ipc::GizmoAxis::None;
        }
        GizmoCommand::ConstrainAxis(axis) => {
            gizmo_state.axis_constraint = axis;
        }
        GizmoCommand::Cancel | GizmoCommand::Confirm => {
            gizmo_state.mode = pentimento_ipc::GizmoMode::None;
            gizmo_state.axis_constraint = pentimento_ipc::GizmoAxis::None;
            gizmo_state.is_active = false;
        }
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
        PaintCommand::SelectBrushPreset { preset_id } => {
            let presets = painting::brush::builtin_presets();
            if let Some(preset) = presets.into_iter().find(|preset| preset.id == preset_id) {
                painting.set_brush_preset(preset);
            }
        }
        PaintCommand::SetBrushColor { color } => {
            painting.set_brush_color(color);
        }
        PaintCommand::SetBrushSize { size } => {
            painting.brush_preset.base_size = size;
            let preset = painting.brush_preset.clone();
            painting.set_brush_preset(preset);
        }
        PaintCommand::SetBrushOpacity { opacity } => {
            painting.brush_preset.opacity = opacity;
            let preset = painting.brush_preset.clone();
            painting.set_brush_preset(preset);
        }
        PaintCommand::SetBrushHardness { hardness } => {
            painting.brush_preset.hardness = hardness;
            let preset = painting.brush_preset.clone();
            painting.set_brush_preset(preset);
        }
        PaintCommand::SetBlendMode { mode } => {
            painting.set_blend_mode_ipc(mode);
        }
        PaintCommand::Undo => {
            let _ = painting.undo_any();
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
