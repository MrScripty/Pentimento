//! IPC message handling between Dioxus UI and Bevy.

use bevy::prelude::*;
use pentimento_ipc::UiToBevy;
use pentimento_scene::OutboundUiMessages;

use super::event_bridge::{BlitzDocumentResource, DioxusBridgeResource};
use crate::render::ui_commands::dispatch_ui_commands;

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

    dispatch_ui_commands(world, messages);
}
