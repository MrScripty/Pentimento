//! Dioxus UI Compositing - Renders Dioxus native UI over the 3D scene
//!
//! This module handles capturing the Dioxus renderer's framebuffer and
//! displaying it as a transparent overlay on top of the Bevy 3D scene.
//!
//! Similar to ui_cef.rs but uses Dioxus (native Rust) instead of Chromium.

use bevy::asset::RenderAssetUsages;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use pentimento_webview::DioxusWebview;

/// Non-Send resource holding the Dioxus renderer
pub struct DioxusRendererResource {
    pub renderer: DioxusWebview,
}

/// Resource holding the UI texture handle
#[derive(Resource)]
pub struct DioxusUiTextureHandle {
    pub handle: Handle<Image>,
}

/// Marker component for the UI overlay node
#[derive(Component)]
pub struct DioxusUiOverlay;

/// Track Dioxus renderer initialization state
#[derive(Resource, Default)]
pub struct DioxusRendererStatus {
    pub initialized: bool,
    pub first_capture_done: bool,
}

/// Resource to track the last known window size
#[derive(Resource, Default)]
pub struct DioxusLastWindowSize {
    pub width: u32,
    pub height: u32,
}

/// Initialize the Dioxus renderer and UI overlay
pub fn setup_ui_dioxus(world: &mut World) {
    // Use PHYSICAL resolution for sharp rendering on HiDPI displays
    let (width, height) = {
        let mut window_query = world.query::<&Window>();
        let window = window_query.iter(world).next().expect("No window found");
        (
            window.resolution.physical_width(),
            window.resolution.physical_height(),
        )
    };

    info!(
        "Setting up Dioxus UI composite system ({}x{} physical)",
        width, height
    );

    // Create the Dioxus renderer
    match DioxusWebview::new((width, height)) {
        Ok(renderer) => {
            world.insert_non_send_resource(DioxusRendererResource { renderer });
            info!("Dioxus renderer created successfully");
        }
        Err(e) => {
            error!("Failed to create Dioxus renderer: {}", e);
            return;
        }
    }

    // Create an initial transparent texture for the UI overlay
    // Use RGBA format - Dioxus/Blitz outputs RGBA (unlike CEF which outputs BGRA)
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0], // Transparent (RGBA)
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    let texture_handle = world.resource_mut::<Assets<Image>>().add(image);

    world.insert_resource(DioxusUiTextureHandle {
        handle: texture_handle.clone(),
    });

    world.insert_resource(DioxusLastWindowSize { width, height });

    // Create a full-screen UI node with the texture
    // Pickable::IGNORE allows raycasts to pass through to 3D meshes for selection
    world.spawn((
        ImageNode {
            image: texture_handle,
            ..default()
        },
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

    info!("Dioxus UI overlay created");
}

/// Update the UI texture from the Dioxus renderer capture
pub fn update_dioxus_ui_texture(
    renderer_res: Option<NonSendMut<DioxusRendererResource>>,
    ui_texture: Option<Res<DioxusUiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut status: ResMut<DioxusRendererStatus>,
) {
    let Some(mut renderer_res) = renderer_res else {
        return;
    };
    let Some(ui_texture) = ui_texture else {
        return;
    };

    // Poll Dioxus event loop
    renderer_res.renderer.poll();

    // Check ready state
    if !renderer_res.renderer.is_ready() {
        return;
    }

    if !status.initialized {
        info!("Dioxus renderer ready, enabling captures");
        status.initialized = true;
    }

    // Try to capture if dirty - returns RGBA bytes with dimensions
    if let Some((rgba_data, cap_width, cap_height)) = renderer_res.renderer.capture_if_dirty() {
        if !status.first_capture_done {
            let non_transparent = rgba_data.chunks(4).filter(|p| p[3] > 0).count();
            info!(
                "First Dioxus capture! {}x{}, non-transparent pixels: {}, format: RGBA",
                cap_width, cap_height, non_transparent
            );
            status.first_capture_done = true;
        }

        if let Some(image) = images.get_mut(&ui_texture.handle) {
            if image.width() != cap_width || image.height() != cap_height {
                info!(
                    "Resizing Dioxus texture from {}x{} to {}x{}",
                    image.width(),
                    image.height(),
                    cap_width,
                    cap_height
                );
                image.resize(Extent3d {
                    width: cap_width,
                    height: cap_height,
                    depth_or_array_layers: 1,
                });
            }

            image.data = Some(rgba_data);
        }
    }
}

/// Handle window resize for Dioxus mode
pub fn handle_dioxus_window_resize(
    renderer_res: Option<NonSendMut<DioxusRendererResource>>,
    ui_texture: Option<Res<DioxusUiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut last_size: ResMut<DioxusLastWindowSize>,
    status: Res<DioxusRendererStatus>,
    windows: Query<&Window>,
) {
    if !status.initialized {
        return;
    }

    let Some(mut renderer_res) = renderer_res else {
        return;
    };
    let Some(ui_texture) = ui_texture else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    // Use PHYSICAL resolution for sharp rendering on HiDPI displays
    let width = window.resolution.physical_width();
    let height = window.resolution.physical_height();

    if width == last_size.width && height == last_size.height {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!(
        "Window resized to {}x{} physical, updating Dioxus renderer",
        width, height
    );
    last_size.width = width;
    last_size.height = height;

    renderer_res.renderer.resize(width, height);

    if let Some(image) = images.get_mut(&ui_texture.handle) {
        image.resize(Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        });
    }
}
