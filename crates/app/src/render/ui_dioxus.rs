//! Dioxus UI Compositing - GPU rendering with Vello
//!
//! This module renders the Dioxus UI using Vello's GPU compute pipeline.
//! Vello renders to a GPU texture which is then displayed as an overlay.
//!
//! # Architecture
//!
//! 1. On startup, we create a Vello renderer in the main world (NonSend)
//! 2. Each frame, Vello renders the UI scene to a texture
//! 3. Bevy displays the texture as a fullscreen overlay
//!
//! Note: Currently uses a CPU-side buffer copy to get the texture into Bevy.
//! Future optimization: direct GPU texture sharing.

use bevy::asset::RenderAssetUsages;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use pentimento_dioxus_ui::{wgpu, DioxusBridge, DioxusBridgeHandle, UiRenderState, VelloRenderer};
use pentimento_ipc::MouseEvent;
use std::sync::Arc;

/// Non-Send resource holding the Vello renderer
/// (Vello's Renderer contains GPU resources that aren't thread-safe)
pub struct VelloRendererWrapper {
    pub renderer: VelloRenderer,
    pub ui_state: UiRenderState,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

/// Resource holding the shared GPU texture handle (main world)
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
    pub first_render_done: bool,
}

/// Resource to track the last known window size
#[derive(Resource, Default)]
pub struct DioxusLastWindowSize {
    pub width: u32,
    pub height: u32,
}

/// Bridge handle for IPC (main world, non-send due to mpsc::Receiver)
pub struct DioxusBridgeResource {
    pub bridge_handle: DioxusBridgeHandle,
}

/// Wrapper for input injection (main world, non-send)
pub struct DioxusRendererResource {
    pub vello: Option<VelloRendererWrapper>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub dirty: bool,
}

impl DioxusRendererResource {
    pub fn new() -> Self {
        Self {
            vello: None,
            mouse_x: 0.0,
            mouse_y: 0.0,
            dirty: true,
        }
    }

    pub fn send_mouse_event(&mut self, event: MouseEvent) {
        match event {
            MouseEvent::Move { x, y } => {
                self.mouse_x = x;
                self.mouse_y = y;
            }
            MouseEvent::ButtonDown { x, y, .. } | MouseEvent::ButtonUp { x, y, .. } => {
                self.mouse_x = x;
                self.mouse_y = y;
                self.dirty = true;
            }
            MouseEvent::Scroll { .. } => {
                self.dirty = true;
            }
        }
    }

    pub fn send_keyboard_event(&mut self, _event: pentimento_ipc::KeyboardEvent) {
        self.dirty = true;
    }
}

/// Initialize the Dioxus/Vello renderer and UI overlay
pub fn setup_ui_dioxus(world: &mut World) {
    // Get window dimensions (physical for HiDPI)
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

    // Create the IPC bridge (non-send due to mpsc::Receiver)
    let (_bridge, bridge_handle) = DioxusBridge::new();
    world.insert_non_send_resource(DioxusBridgeResource { bridge_handle });

    // Create input handler (non-send resource)
    world.insert_non_send_resource(DioxusRendererResource::new());

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

    info!("Dioxus UI overlay created (Vello renderer will initialize on first frame)");
}

/// Initialize the Vello renderer lazily on first frame
/// This is needed because we need a wgpu device, which we create ourselves
fn init_vello_renderer(width: u32, height: u32) -> Result<VelloRendererWrapper, String> {
    info!("Initializing Vello renderer {}x{}", width, height);

    // Create our own wgpu instance, adapter, device, queue
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|e| format!("Failed to find suitable adapter: {:?}", e))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("vello_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .map_err(|e| format!("Failed to create device: {:?}", e))?;

    let device = Arc::new(device);
    let queue = Arc::new(queue);

    // Create Vello renderer
    let renderer = VelloRenderer::new_with_device(&device, width, height)
        .map_err(|e| format!("Failed to create Vello renderer: {}", e))?;

    // Create texture for rendering
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vello_ui_texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    info!("Vello renderer initialized successfully");

    Ok(VelloRendererWrapper {
        renderer,
        ui_state: UiRenderState::default(),
        device,
        queue,
        texture,
        texture_view,
        width,
        height,
    })
}

/// Update the UI texture from the Vello renderer
pub fn update_dioxus_ui_texture(
    mut renderer_res: NonSendMut<DioxusRendererResource>,
    ui_texture: Option<Res<DioxusUiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut status: ResMut<DioxusRendererStatus>,
    last_size: Res<DioxusLastWindowSize>,
) {
    let Some(ui_texture) = ui_texture else {
        return;
    };

    let width = last_size.width;
    let height = last_size.height;

    // Initialize Vello renderer if not done yet
    if renderer_res.vello.is_none() {
        match init_vello_renderer(width, height) {
            Ok(vello) => {
                renderer_res.vello = Some(vello);
                status.initialized = true;
                info!("Vello renderer ready");
            }
            Err(e) => {
                error!("Failed to initialize Vello: {}", e);
                return;
            }
        }
    }

    let Some(vello) = renderer_res.vello.as_mut() else {
        return;
    };

    // Build and render the scene
    vello.renderer.build_scene(&vello.ui_state);

    if let Err(e) = vello
        .renderer
        .render_to_texture(&vello.device, &vello.queue, &vello.texture_view)
    {
        error!("Vello render failed: {}", e);
        return;
    }

    // Read back texture data (this is the CPU copy we want to eliminate later)
    let bytes_per_row = width * 4;
    let padded_bytes_per_row = (bytes_per_row + 255) & !255; // Align to 256
    let buffer_size = padded_bytes_per_row * height;

    let staging_buffer = vello.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vello_staging"),
        size: buffer_size as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = vello
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vello_copy"),
        });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &vello.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    vello.queue.submit(std::iter::once(encoder.finish()));

    // Map and read the buffer
    let buffer_slice = staging_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    let _ = vello.device.poll(wgpu::PollType::wait_indefinitely());

    if rx.recv().unwrap().is_ok() {
        let data = buffer_slice.get_mapped_range();

        // Copy to Bevy image, removing padding
        if let Some(image) = images.get_mut(&ui_texture.handle) {
            let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
            for row in 0..height {
                let start = (row * padded_bytes_per_row) as usize;
                let end = start + (width * 4) as usize;
                rgba_data.extend_from_slice(&data[start..end]);
            }
            image.data = Some(rgba_data);

            if !status.first_render_done {
                info!("First Vello render completed (GPU → CPU → Bevy texture)");
                status.first_render_done = true;
            }
        }

        drop(data);
        staging_buffer.unmap();
    }
}

/// Handle window resize for Dioxus mode
pub fn handle_dioxus_window_resize(
    mut renderer_res: NonSendMut<DioxusRendererResource>,
    ui_texture: Option<Res<DioxusUiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut last_size: ResMut<DioxusLastWindowSize>,
    status: Res<DioxusRendererStatus>,
    windows: Query<&Window>,
) {
    if !status.initialized {
        return;
    }

    let Some(ui_texture) = ui_texture else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let width = window.resolution.physical_width();
    let height = window.resolution.physical_height();

    if width == last_size.width && height == last_size.height {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!(
        "Window resized to {}x{} physical, updating Vello renderer",
        width, height
    );
    last_size.width = width;
    last_size.height = height;

    // Resize Bevy image
    if let Some(image) = images.get_mut(&ui_texture.handle) {
        image.resize(Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        });
    }

    // Resize Vello renderer and texture
    if let Some(vello) = renderer_res.vello.as_mut() {
        vello.renderer.resize(width, height);
        vello.width = width;
        vello.height = height;

        // Recreate texture at new size
        vello.texture = vello.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vello_ui_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        vello.texture_view = vello
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
    }
}
