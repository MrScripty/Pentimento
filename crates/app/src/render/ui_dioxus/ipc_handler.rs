//! IPC message handling between Dioxus UI and Bevy.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use painting::PaintingPipeline;
use pentimento_ipc::{BevyToUi, LayerInfo, PaintCommand, UiToBevy};
use pentimento_scene::{
    ActiveCanvasPlane, AddObjectEvent, CanvasPlane, CanvasPlaneEvent, DepthViewSettings,
    OutboundUiMessages, PaintingResource, SceneAmbientOcclusion, SceneLighting,
};

use super::event_bridge::{BlitzDocumentResource, DioxusBridgeResource};

/// Handle IPC messages from the Dioxus UI and dispatch to appropriate Bevy events.
/// This is an exclusive system because DioxusBridgeResource is NonSend.
pub fn handle_ui_to_bevy_messages(world: &mut World) {
    // Forward outbound messages (Bevy->UI) first
    let outbound_msgs = {
        if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
            outbound.drain()
        } else {
            Vec::new()
        }
    };

    if !outbound_msgs.is_empty() {
        if let Some(bridge) = world.get_non_send_resource::<DioxusBridgeResource>() {
            for msg in &outbound_msgs {
                eprintln!(">>> IPC forwarding outbound to UI: {:?}", msg);
            }
            for msg in outbound_msgs {
                bridge.bridge_handle.send(msg);
            }
        } else {
            warn!("No DioxusBridgeResource found!");
        }

        // Mark scope dirty and poll to trigger incremental re-render.
        // Uses render_immediate() (incremental diffing), not rebuild() (appends nodes).
        // IPC messages are read by the component during render.
        if let Some(mut doc_resource) = world.get_non_send_resource_mut::<BlitzDocumentResource>() {
            doc_resource.document.mark_dirty_and_poll();
        }
    }

    // Collect all pending messages first to avoid holding the borrow
    let messages: Vec<UiToBevy> = {
        let Some(bridge) = world.get_non_send_resource::<DioxusBridgeResource>() else {
            return;
        };
        let mut msgs = Vec::new();
        while let Some(msg) = bridge.bridge_handle.try_recv() {
            msgs.push(msg);
        }
        msgs
    };

    if messages.is_empty() {
        return;
    }

    // Get active canvas plane_id for layer commands
    let active_plane_id: Option<u32> = world
        .get_resource::<ActiveCanvasPlane>()
        .and_then(|ap| ap.entity)
        .and_then(|e| world.get::<CanvasPlane>(e))
        .map(|cp| cp.plane_id);

    // Process each message, collecting events to send
    let mut canvas_events: Vec<CanvasPlaneEvent> = Vec::new();
    let mut outbound_layer_msgs: Vec<BevyToUi> = Vec::new();

    for msg in messages {
        match msg {
            UiToBevy::AddPaintCanvas(request) => {
                canvas_events.push(CanvasPlaneEvent::CreateInFrontOfCamera {
                    width: request.width.unwrap_or(1024),
                    height: request.height.unwrap_or(1024),
                });
                info!("Received AddPaintCanvas request, creating canvas in front of camera");
            }
            UiToBevy::UiDirty => {
                // UI has changed - in Dioxus mode this is handled by the Vello renderer
            }
            UiToBevy::PaintCommand(cmd) => {
                if let Some(mut painting_res) = world.get_resource_mut::<PaintingResource>() {
                    match cmd {
                        PaintCommand::SelectBrushPreset { preset_id } => {
                            let presets = painting::brush::builtin_presets();
                            if let Some(preset) = presets.into_iter().find(|p| p.id == preset_id) {
                                painting_res.set_brush_preset(preset);
                                info!("Selected brush preset: id={}", preset_id);
                            }
                        }
                        PaintCommand::SetBrushColor { color } => {
                            painting_res.set_brush_color(color);
                            debug!("Set brush color to {:?}", color);
                        }
                        PaintCommand::SetBrushSize { size } => {
                            painting_res.brush_preset.base_size = size;
                            let preset = painting_res.brush_preset.clone();
                            painting_res.set_brush_preset(preset);
                            debug!("Set brush size to {}", size);
                        }
                        PaintCommand::SetBrushOpacity { opacity } => {
                            painting_res.brush_preset.opacity = opacity;
                            let preset = painting_res.brush_preset.clone();
                            painting_res.set_brush_preset(preset);
                            debug!("Set brush opacity to {}", opacity);
                        }
                        PaintCommand::SetBrushHardness { hardness } => {
                            painting_res.brush_preset.hardness = hardness;
                            let preset = painting_res.brush_preset.clone();
                            painting_res.set_brush_preset(preset);
                            debug!("Set brush hardness to {}", hardness);
                        }
                        PaintCommand::SetBlendMode { mode } => {
                            painting_res.set_blend_mode_ipc(mode);
                            debug!("Set blend mode to {:?}", mode);
                        }
                        PaintCommand::Undo => {
                            if painting_res.undo_any() {
                                info!("Paint undo performed");
                            } else {
                                debug!("Paint undo: nothing to undo");
                            }
                        }
                        PaintCommand::SetLiveProjection { enabled } => {
                            debug!("Set live projection to {}", enabled);
                            // TODO: Implement live projection toggle
                        }
                        PaintCommand::ProjectToScene => {
                            debug!("Project to scene requested");
                            // TODO: Implement one-shot projection
                        }
                        PaintCommand::AddLayer { name } => {
                            if let Some(plane_id) = active_plane_id {
                                if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                                    let id = pipeline.layers.add_layer(name);
                                    outbound_layer_msgs.push(make_layer_state_msg(pipeline));
                                    info!("Added layer {} on plane {}", id, plane_id);
                                }
                            }
                        }
                        PaintCommand::RemoveLayer { layer_id } => {
                            if let Some(plane_id) = active_plane_id {
                                if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                                    if pipeline.layers.remove_layer(layer_id) {
                                        outbound_layer_msgs.push(make_layer_state_msg(pipeline));
                                        info!("Removed layer {} on plane {}", layer_id, plane_id);
                                    }
                                }
                            }
                        }
                        PaintCommand::SetActiveLayer { layer_id } => {
                            if let Some(plane_id) = active_plane_id {
                                if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                                    if pipeline.layers.set_active(layer_id) {
                                        outbound_layer_msgs.push(make_layer_state_msg(pipeline));
                                        info!("Set active layer {} on plane {}", layer_id, plane_id);
                                    }
                                }
                            }
                        }
                        PaintCommand::SetLayerVisibility { layer_id, visible } => {
                            if let Some(plane_id) = active_plane_id {
                                if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                                    pipeline.layers.set_visibility(layer_id, visible);
                                    outbound_layer_msgs.push(make_layer_state_msg(pipeline));
                                    info!("Set layer {} visibility={} on plane {}", layer_id, visible, plane_id);
                                }
                            }
                        }
                        PaintCommand::SetLayerOpacity { layer_id, opacity } => {
                            if let Some(plane_id) = active_plane_id {
                                if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                                    pipeline.layers.set_opacity(layer_id, opacity);
                                    outbound_layer_msgs.push(make_layer_state_msg(pipeline));
                                    info!("Set layer {} opacity={:.2} on plane {}", layer_id, opacity, plane_id);
                                }
                            }
                        }
                        PaintCommand::ReorderLayer { layer_id, new_index } => {
                            if let Some(plane_id) = active_plane_id {
                                if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                                    pipeline.layers.reorder(layer_id, new_index as usize);
                                    outbound_layer_msgs.push(make_layer_state_msg(pipeline));
                                    info!("Reordered layer {} to index {} on plane {}", layer_id, new_index, plane_id);
                                }
                            }
                        }
                        PaintCommand::RenameLayer { layer_id, name } => {
                            if let Some(plane_id) = active_plane_id {
                                if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                                    pipeline.layers.rename(layer_id, name);
                                    outbound_layer_msgs.push(make_layer_state_msg(pipeline));
                                    info!("Renamed layer {} on plane {}", layer_id, plane_id);
                                }
                            }
                        }
                    }
                }
            }
            UiToBevy::AddObject(request) => {
                if let Some(mut events) = world.get_resource_mut::<Messages<AddObjectEvent>>() {
                    events.write(AddObjectEvent(request));
                    info!("Dispatched AddObjectEvent from Dioxus UI");
                }
            }
            UiToBevy::UpdateAmbientOcclusion(settings) => {
                if let Some(mut ao_resource) = world.get_resource_mut::<SceneAmbientOcclusion>() {
                    ao_resource.update(settings);
                    info!("Updated ambient occlusion settings from UI");
                }
            }
            UiToBevy::UpdateLighting(settings) => {
                if let Some(mut lighting) = world.get_resource_mut::<SceneLighting>() {
                    lighting.settings = settings;
                    info!("Updated lighting settings from UI");
                }
            }
            UiToBevy::SetDepthView { enabled } => {
                if let Some(mut settings) = world.get_resource_mut::<DepthViewSettings>() {
                    settings.enabled = enabled;
                    info!("Depth view mode: {}", if enabled { "enabled" } else { "disabled" });
                }
            }
            _ => {
                // Other messages not yet implemented
                debug!("Received unhandled UI message: {:?}", msg);
            }
        }
    }

    // Send collected canvas events
    if !canvas_events.is_empty() {
        if let Some(mut messages) = world.get_resource_mut::<Messages<CanvasPlaneEvent>>() {
            for event in canvas_events {
                messages.write(event);
            }
        }
    }

    // Send layer state messages to UI (forwarded to bridge on next frame)
    if !outbound_layer_msgs.is_empty() {
        if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
            for msg in outbound_layer_msgs {
                outbound.send(msg);
            }
        }
    }
}

/// Convert pipeline layer info to IPC LayerInfo and wrap in a BevyToUi message
fn make_layer_state_msg(pipeline: &PaintingPipeline) -> BevyToUi {
    let layers = pipeline
        .layers
        .layer_info()
        .into_iter()
        .map(|l| LayerInfo {
            id: l.id,
            name: l.name,
            visible: l.visible,
            opacity: l.opacity,
            is_active: l.is_active,
        })
        .collect();
    BevyToUi::LayerStateChanged { layers }
}
