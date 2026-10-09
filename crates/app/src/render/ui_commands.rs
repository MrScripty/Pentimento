use bevy::ecs::message::Messages;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use pentimento_ipc::{CameraCommand, GizmoCommand, MaterialCommand, ObjectCommand, UiToBevy};
use pentimento_scene::{
    AddObjectEvent, CanvasPlaneEvent, DepthViewSettings, GizmoState, MainCamera, MeshEditEvent,
    OrbitCamera, SceneAmbientOcclusion, SceneLighting,
};
#[cfg(feature = "selection")]
use pentimento_scene::{Selectable, Selected, SelectionState};

pub(crate) fn dispatch_ui_commands(
    world: &mut World,
    commands: impl IntoIterator<Item = UiToBevy>,
) {
    let mut canvas_events = Vec::new();

    for command in commands {
        if matches!(
            &command,
            UiToBevy::ProjectCommand(
                pentimento_ipc::ProjectCommand::Save { .. }
                    | pentimento_ipc::ProjectCommand::New { .. }
            )
        ) {
            if let Some(mut messages) = world.get_resource_mut::<Messages<CanvasPlaneEvent>>() {
                for event in canvas_events.drain(..) {
                    messages.write(event);
                }
            }
        }
        let generation = pentimento_scene::project_generation(world);
        if pentimento_scene::dispatch_brush_ui_command(world, &command) {
            if generation != pentimento_scene::project_generation(world) {
                canvas_events.clear();
                break;
            }
            continue;
        }
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
            UiToBevy::MeshEditCommand(command) => handle_mesh_edit_command(world, command),
            UiToBevy::GizmoCommand(command) => handle_gizmo_command(world, command),
            UiToBevy::PaintCommand(_)
            | UiToBevy::ProjectCommand(_)
            | UiToBevy::SculptCommand(_)
            | UiToBevy::RequestBrushState
            | UiToBevy::SetUiInputCapture { .. }
            | UiToBevy::UiDirty
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

#[cfg(test)]
mod project_command_tests {
    use super::*;
    use pentimento_ipc::ProjectCommand;
    #[test]
    fn save_refuses_earlier_unapplied_object_and_canvas_commands_in_native_batch() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<pentimento_scene::PaintingResource>();
        world.init_resource::<pentimento_scene::OutboundUiMessages>();
        world.init_resource::<Messages<CanvasPlaneEvent>>();
        world.init_resource::<Messages<AddObjectEvent>>();
        let directory = std::env::temp_dir().join(format!(
            "pentimento-native-project-save-order-{}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory
            .join("owned.pentimento.json")
            .to_string_lossy()
            .into_owned();
        dispatch_ui_commands(
            &mut world,
            [
                UiToBevy::AddObject(pentimento_ipc::AddObjectRequest {
                    primitive_type: pentimento_ipc::PrimitiveType::Cube,
                    position: None,
                    name: None,
                }),
                UiToBevy::AddPaintCanvas(pentimento_ipc::AddPaintCanvasRequest {
                    width: Some(2),
                    height: Some(2),
                }),
                UiToBevy::ProjectCommand(ProjectCommand::Save { path: path.clone() }),
            ],
        );
        assert!(!std::path::Path::new(&path).exists());
        assert!(!world.resource::<Messages<CanvasPlaneEvent>>().is_empty());
        assert!(!world.resource::<Messages<AddObjectEvent>>().is_empty());
        assert_eq!(pentimento_scene::project_generation(&world), 0);
        assert!(world.resource::<pentimento_scene::OutboundUiMessages>().messages.iter().any(|m|matches!(m,pentimento_ipc::BevyToUi::ProjectOperationFinished{operation,success:false,..} if operation=="Save")));
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn successful_open_discards_prefix_queued_actions_and_suffix_commands_in_native_batch() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<pentimento_scene::PaintingResource>();
        world.init_resource::<pentimento_scene::OutboundUiMessages>();
        world.init_resource::<Messages<CanvasPlaneEvent>>();
        world.init_resource::<Messages<AddObjectEvent>>();
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::new(2., 2.));
        let mat = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        world.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default()));
        let directory = std::env::temp_dir().join(format!(
            "pentimento-native-project-batch-{}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory
            .join("owned.pentimento.json")
            .to_string_lossy()
            .into_owned();
        dispatch_ui_commands(
            &mut world,
            [UiToBevy::ProjectCommand(ProjectCommand::Save {
                path: path.clone(),
            })],
        );
        assert!(std::path::Path::new(&path).is_file());
        let add = || {
            UiToBevy::AddObject(pentimento_ipc::AddObjectRequest {
                primitive_type: pentimento_ipc::PrimitiveType::Cube,
                position: None,
                name: None,
            })
        };
        dispatch_ui_commands(
            &mut world,
            [
                UiToBevy::AddPaintCanvas(pentimento_ipc::AddPaintCanvasRequest {
                    width: Some(2),
                    height: Some(2),
                }),
                add(),
                UiToBevy::ProjectCommand(ProjectCommand::Open { path }),
                UiToBevy::AddPaintCanvas(pentimento_ipc::AddPaintCanvasRequest {
                    width: Some(4),
                    height: Some(4),
                }),
                add(),
            ],
        );
        assert_eq!(pentimento_scene::project_generation(&world), 1);
        assert!(world.resource::<Messages<CanvasPlaneEvent>>().is_empty());
        assert!(world.resource::<Messages<AddObjectEvent>>().is_empty());
        assert_eq!(world.query::<&Mesh3d>().iter(&world).count(), 1);
        assert!(world.resource::<pentimento_scene::OutboundUiMessages>().messages.iter().any(|m|matches!(m,pentimento_ipc::BevyToUi::ProjectOperationFinished{operation,success:true,..} if operation=="Open")));
        std::fs::remove_dir_all(directory).unwrap();
    }
    fn new_world() -> World {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<pentimento_scene::PaintingResource>();
        world.init_resource::<pentimento_scene::OutboundUiMessages>();
        world.init_resource::<Messages<CanvasPlaneEvent>>();
        world.init_resource::<Messages<AddObjectEvent>>();
        world
    }
    fn new_command() -> UiToBevy {
        UiToBevy::ProjectCommand(ProjectCommand::New {
            expected_generation: "0".into(),
            confirm_discard: true,
        })
    }
    #[test]
    fn new_refuses_each_unapplied_native_prefix_including_locally_buffered_canvas() {
        for prefix in [
            UiToBevy::AddPaintCanvas(pentimento_ipc::AddPaintCanvasRequest {
                width: Some(2),
                height: Some(2),
            }),
            UiToBevy::AddObject(pentimento_ipc::AddObjectRequest {
                primitive_type: pentimento_ipc::PrimitiveType::Cube,
                position: None,
                name: None,
            }),
        ] {
            let mut world = new_world();
            dispatch_ui_commands(&mut world, [prefix, new_command()]);
            assert_eq!(pentimento_scene::project_generation(&world), 0);
            assert!(world.resource::<pentimento_scene::OutboundUiMessages>().messages.iter().any(|m|matches!(m,pentimento_ipc::BevyToUi::ProjectOperationFinished {operation,success:false,..} if operation=="New")));
            assert!(
                !world.resource::<Messages<CanvasPlaneEvent>>().is_empty()
                    || !world.resource::<Messages<AddObjectEvent>>().is_empty()
            );
        }
    }
    #[test]
    fn successful_new_drops_old_batch_suffix_and_subsequent_save_as_works() {
        let mut world = new_world();
        let directory =
            std::env::temp_dir().join(format!("pentimento-native-new-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let old = directory
            .join("old.pentimento.json")
            .to_string_lossy()
            .into_owned();
        let fresh = directory
            .join("fresh.pentimento.json")
            .to_string_lossy()
            .into_owned();
        dispatch_ui_commands(
            &mut world,
            [UiToBevy::ProjectCommand(ProjectCommand::Save {
                path: old.clone(),
            })],
        );
        let bytes = std::fs::read(&old).unwrap();
        dispatch_ui_commands(
            &mut world,
            [
                new_command(),
                UiToBevy::AddPaintCanvas(pentimento_ipc::AddPaintCanvasRequest {
                    width: Some(4),
                    height: Some(4),
                }),
                UiToBevy::ProjectCommand(ProjectCommand::Save { path: old.clone() }),
                UiToBevy::AddObject(pentimento_ipc::AddObjectRequest {
                    primitive_type: pentimento_ipc::PrimitiveType::Cube,
                    position: None,
                    name: None,
                }),
            ],
        );
        assert_eq!(pentimento_scene::project_generation(&world), 1);
        assert!(world.resource::<Messages<CanvasPlaneEvent>>().is_empty());
        assert!(world.resource::<Messages<AddObjectEvent>>().is_empty());
        assert_eq!(std::fs::read(&old).unwrap(), bytes);
        assert!(world.resource::<pentimento_scene::OutboundUiMessages>().messages.iter().any(|m|matches!(m,pentimento_ipc::BevyToUi::ProjectStateChanged {path:None,generation,..} if generation=="1")));
        dispatch_ui_commands(
            &mut world,
            [UiToBevy::ProjectCommand(ProjectCommand::Save {
                path: fresh.clone(),
            })],
        );
        assert!(std::path::Path::new(&fresh).is_file());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
