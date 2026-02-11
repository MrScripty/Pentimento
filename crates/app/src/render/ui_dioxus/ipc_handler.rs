//! IPC message handling between Dioxus UI and Bevy.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use pentimento_ipc::{PaintCommand, UiToBevy};
use pentimento_scene::{
    AddObjectEvent, CanvasPlaneEvent, DepthViewSettings, OutboundUiMessages, PaintingResource,
    SceneAmbientOcclusion, SceneLighting,
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

    // Process each message, collecting events to send
    let mut canvas_events: Vec<CanvasPlaneEvent> = Vec::new();

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
}
