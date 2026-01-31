//! UI scene building and window resize handling.

use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use pentimento_dioxus_ui::{BlitzDocument, UiEvent};
use pentimento_scene::OutboundUiMessages;

use super::event_bridge::{BlitzDocumentResource, DioxusEventReceiver};
use super::resources::{
    DioxusRenderTarget, DioxusSetupStatus, DioxusUiState, VelloSceneBuffer,
};

/// Build the UI scene from BlitzDocument (runs every frame in main world).
/// This is an exclusive system because BlitzDocumentResource is NonSend.
pub fn build_ui_scene(world: &mut World) {
    // Skip if setup hasn't completed yet (receiver doesn't exist until then)
    {
        let status = world.resource::<DioxusSetupStatus>();
        if !status.setup_done {
            return;
        }
    }

    // Process network and document messages first (asset loading, head elements)
    // This ensures resources are loaded before the UI tries to use them
    {
        if let Some(mut doc_resource) = world.get_non_send_resource_mut::<BlitzDocumentResource>() {
            doc_resource.document.process_messages();
        }
    }

    // Drain queued input events from the channel receiver
    let events: Vec<UiEvent> = {
        if let Some(receiver) = world.get_non_send_resource::<DioxusEventReceiver>() {
            receiver.0.try_iter().collect()
        } else {
            // This shouldn't happen since we check setup_done above
            Vec::new()
        }
    };

    // Process input events and poll the document (needs mutable access, separate scope)
    let viewport_clicked = {
        let Some(mut doc_resource) = world.get_non_send_resource_mut::<BlitzDocumentResource>()
        else {
            return;
        };

        // Always poll to process any pending state changes from IPC messages.
        // This is critical: IPC messages (like ShowAddObjectMenu) update shared state,
        // and the component needs to poll to read that state and update its signals.
        // Without this, state changes from handle_ui_to_bevy_messages wouldn't be reflected.
        doc_resource.document.poll();

        // Forward queued events to BlitzDocument
        for event in &events {
            doc_resource.document.handle_event(event.clone());
        }

        // Don't call force_render() here - poll() already handles reactive updates.
        // force_render() uses rebuild() which APPENDS nodes, causing UI duplication.
        // Signal changes in event handlers automatically mark scopes dirty for poll().

        // Check if a viewport click occurred (click outside UI elements)
        doc_resource.document.take_viewport_clicked()
    };

    // If viewport was clicked, notify UI to close menus
    if viewport_clicked {
        if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
            outbound.send(pentimento_ipc::BevyToUi::CloseMenus);
        }
    }

    // Get a raw pointer to the document for painting
    // SAFETY: We only hold an immutable reference to the document while mutating the scene buffer.
    // The document and scene buffer are independent resources with no aliasing.
    let doc_ptr = {
        let Some(doc_resource) = world.get_non_send_resource::<BlitzDocumentResource>() else {
            return;
        };
        &doc_resource.document as *const BlitzDocument
    };

    let Some(mut scene_buffer) = world.get_resource_mut::<VelloSceneBuffer>() else {
        return;
    };

    // SAFETY: doc_ptr points to valid data that outlives this scope.
    // BlitzDocument::paint_to_scene only requires &self (immutable).
    unsafe {
        (*doc_ptr).paint_to_scene(&mut scene_buffer.scene);
    }
}

/// Handle window resize - update texture, UI state, and BlitzDocument.
/// This is an exclusive system because BlitzDocumentResource is NonSend.
pub fn handle_window_resize(world: &mut World) {
    // Check if window changed
    // Use LOGICAL dimensions to match initial setup and mouse coordinates
    let (width, height, _changed) = {
        let mut query = world.query_filtered::<&Window, Changed<Window>>();
        match query.iter(world).next() {
            Some(window) => (
                window.resolution.width() as u32,  // logical width
                window.resolution.height() as u32, // logical height
                true,
            ),
            None => return,
        }
    };

    if width == 0 || height == 0 {
        return;
    }

    // Check if size actually changed
    let current_size = {
        world
            .get_resource::<DioxusUiState>()
            .map(|s| (s.width, s.height))
    };

    if let Some((cur_w, cur_h)) = current_size {
        if cur_w == width && cur_h == height {
            return;
        }
    }

    info!(
        "Window resized to {}x{} logical, updating UI texture",
        width, height
    );

    // Update UI state
    if let Some(mut ui_state) = world.get_resource_mut::<DioxusUiState>() {
        ui_state.width = width;
        ui_state.height = height;
    }

    // Resize the BlitzDocument
    if let Some(mut doc_resource) = world.get_non_send_resource_mut::<BlitzDocumentResource>() {
        doc_resource.document.resize(width, height);
    }

    // Resize the Bevy Image asset
    let handle = world
        .get_resource::<DioxusRenderTarget>()
        .map(|rt| rt.handle.clone());
    if let Some(handle) = handle {
        if let Some(mut images) = world.get_resource_mut::<Assets<Image>>() {
            if let Some(image) = images.get_mut(&handle) {
                image.resize(Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                });
            }
        }
    }
}
