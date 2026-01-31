//! Dioxus UI texture setup and initialization.

use bevy::asset::RenderAssetUsages;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use pentimento_dioxus_ui::{BlitzDocument, DioxusBridge};

use super::event_bridge::{
    create_event_channel, BlitzDocumentResource, DioxusBridgeResource, DioxusRendererResource,
};
use super::resources::{
    DioxusRenderTarget, DioxusRenderTargetId, DioxusSetupStatus, DioxusUiOverlay, DioxusUiState,
};
use crate::render::ui_blend_material::UiBlendMaterial;

/// Deferred setup that waits for window size to stabilize before initializing.
/// This prevents issues where the window starts at one size and immediately resizes.
pub fn deferred_setup_dioxus_texture(world: &mut World) {
    // Check if already set up
    {
        let status = world.resource::<DioxusSetupStatus>();
        if status.setup_done {
            return;
        }
    }

    // Wait 2 frames for window size to stabilize
    const FRAMES_TO_WAIT: u32 = 2;
    {
        let mut status = world.resource_mut::<DioxusSetupStatus>();
        status.frames_waited += 1;
        if status.frames_waited < FRAMES_TO_WAIT {
            return;
        }
    }

    // Now run the actual setup
    setup_dioxus_texture(world);

    // Mark as done
    world.resource_mut::<DioxusSetupStatus>().setup_done = true;
}

/// Initialize the Dioxus UI texture, BlitzDocument, and overlay node.
/// This is an exclusive system because BlitzDocumentResource is NonSend.
pub fn setup_dioxus_texture(world: &mut World) {
    // Get window dimensions in LOGICAL pixels
    // We use logical coordinates throughout to match Bevy's mouse event coordinates
    let (width, height) = {
        let mut window_query = world.query::<&Window>();
        let Some(window) = window_query.iter(world).next() else {
            error!("No window found for Dioxus UI setup");
            return;
        };
        (
            window.resolution.width() as u32,  // logical width
            window.resolution.height() as u32, // logical height
        )
    };
    // Use scale_factor of 1.0 since we're working in logical pixels
    let scale_factor = 1.0_f64;

    // Get the actual scale factor for logging
    let actual_scale = {
        let mut window_query = world.query::<&Window>();
        window_query
            .iter(world)
            .next()
            .map(|w| w.resolution.scale_factor())
            .unwrap_or(1.0)
    };
    info!(
        "Setting up Dioxus UI texture: {}x{} logical (device scale={}, using scale=1.0)",
        width, height, actual_scale
    );

    // Create the IPC bridge (non-send due to mpsc::Receiver)
    let (bridge, bridge_handle) = DioxusBridge::new();
    world.insert_non_send_resource(DioxusBridgeResource { bridge_handle });

    // Create the UI event channel for input forwarding
    let (event_sender, event_receiver) = create_event_channel();
    world.insert_non_send_resource(DioxusRendererResource::new(event_sender));
    world.insert_non_send_resource(event_receiver);

    // Create the BlitzDocument with our Dioxus UI components
    let document = BlitzDocument::new(width, height, scale_factor, bridge);
    world.insert_non_send_resource(BlitzDocumentResource { document });

    // Create a Bevy Image for the UI texture
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0], // Transparent (RGBA)
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD, // Only needed in render world
    );

    // CRITICAL: STORAGE_BINDING is required for Vello's compute shaders
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::RENDER_ATTACHMENT
        | TextureUsages::STORAGE_BINDING;

    let handle = world.resource_mut::<Assets<Image>>().add(image);

    // Store resources
    world.insert_resource(DioxusRenderTarget {
        handle: handle.clone(),
    });
    world.insert_resource(DioxusRenderTargetId(handle.id()));
    world.insert_resource(DioxusUiState { width, height });

    // Create the blend material for proper alpha compositing
    let material_handle = world
        .resource_mut::<Assets<UiBlendMaterial>>()
        .add(UiBlendMaterial {
            texture: handle,
        });

    // Create a full-screen UI node with the custom blend material
    // MaterialNode ensures proper alpha blending over the 3D scene
    world.spawn((
        MaterialNode(material_handle),
        Node {
            width: Val::Vw(100.0),
            height: Val::Vh(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        ZIndex(i32::MAX),
        DioxusUiOverlay,
        Pickable::IGNORE,
    ));

    info!("Dioxus UI overlay created with Blitz+Vello rendering");
}
