use bevy::ecs::message::Messages;
use bevy::prelude::*;
use painting::PaintingPipeline;
use pentimento_egui_ui::apply_bevy_message;
use pentimento_ipc::{BevyToUi, LayerInfo, PaintCommand, UiToBevy};
use pentimento_scene::{
    ActiveCanvasPlane, AddObjectEvent, CanvasPlane, CanvasPlaneEvent, DepthViewSettings,
    OutboundUiMessages, PaintingResource, SceneAmbientOcclusion, SceneLighting,
};

use super::resources::EguiFrontendState;

pub fn sync_bevy_messages(
    mut outbound: ResMut<OutboundUiMessages>,
    mut frontend: ResMut<EguiFrontendState>,
) {
    for message in outbound.drain() {
        apply_bevy_message(&mut frontend.snapshot, message);
    }
}

pub fn dispatch_ui_commands(world: &mut World, commands: impl IntoIterator<Item = UiToBevy>) {
    let active_plane_id: Option<u32> = world
        .get_resource::<ActiveCanvasPlane>()
        .and_then(|active| active.entity)
        .and_then(|entity| world.get::<CanvasPlane>(entity))
        .map(|canvas| canvas.plane_id);

    let mut canvas_events = Vec::new();
    let mut outbound_layer_messages = Vec::new();

    for message in commands {
        match message {
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
            _ => {
                debug!("Unhandled egui UI message: {:?}", message);
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
            debug!("egui live projection toggle requested: {}", enabled);
        }
        PaintCommand::ProjectToScene => {
            debug!("egui project-to-scene requested");
        }
        PaintCommand::AddLayer { name } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                let _ = pipeline.layers.add_layer(name);
                outbound_layer_messages.push(make_layer_state_message(pipeline));
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
        PaintCommand::SelectBrushPreset { preset_id } => {
            let presets = painting::brush::builtin_presets();
            if let Some(preset) = presets.into_iter().find(|preset| preset.id == preset_id) {
                painting.set_brush_preset(preset);
            }
        }
        PaintCommand::SetBrushColor { color } => {
            painting.set_brush_color(color);
        }
        PaintCommand::RemoveLayer { layer_id } => {
            with_active_pipeline(active_plane_id, &mut painting, |pipeline| {
                if pipeline.layers.remove_layer(layer_id) {
                    outbound_layer_messages.push(make_layer_state_message(pipeline));
                }
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
