//! Dioxus UI Compositing - Zero-Copy GPU rendering with Vello
//!
//! This module renders the Dioxus UI using Vello's GPU compute pipeline directly
//! to a Bevy-owned texture, eliminating CPU-side copies.
//!
//! # Architecture
//!
//! 1. Main world: UI state is extracted to render world each frame
//! 2. Render world (Prepare): Vello scene graph is built from UI state
//! 3. Render world (Render): Vello renders directly to Bevy's GpuImage
//! 4. Bevy composites the texture over the 3D scene
//!
//! # Thread Safety
//!
//! Vello's Renderer is wrapped in `Arc<Mutex<...>>` to be Send+Sync for
//! the render world. The mutex is only held during the single render call
//! per frame, so there's no contention.

use bevy::asset::{AssetId, RenderAssetUsages};
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderSystems};
use pentimento_dioxus_ui::{
    kurbo, peniko, AaConfig, DioxusBridge, DioxusBridgeHandle, RenderParams, Scene,
    SharedVelloRenderer,
};
use pentimento_ipc::MouseEvent;

// ============================================================================
// Main World Resources
// ============================================================================

/// UI state that gets extracted to the render world each frame.
/// This contains all the data needed to build the Vello scene.
#[derive(Resource, Clone, ExtractResource, Default)]
pub struct DioxusUiState {
    pub fps: f32,
    pub frame_time: f32,
    pub selected_tool: String,
    pub width: u32,
    pub height: u32,
}

/// Handle to the render target texture (extracted to render world via AssetId).
#[derive(Resource, Clone)]
pub struct DioxusRenderTarget {
    pub handle: Handle<Image>,
}

/// Extractable version that just carries the AssetId.
#[derive(Resource, Clone, ExtractResource)]
pub struct DioxusRenderTargetId(pub AssetId<Image>);

/// Marker component for the UI overlay node.
#[derive(Component)]
pub struct DioxusUiOverlay;

/// Bridge handle for IPC (main world, non-send due to mpsc::Receiver).
pub struct DioxusBridgeResource {
    pub bridge_handle: DioxusBridgeHandle,
}

/// Input state for the Dioxus UI (main world).
#[derive(Resource, Default)]
pub struct DioxusInputState {
    pub mouse_x: f32,
    pub mouse_y: f32,
}

impl DioxusInputState {
    pub fn send_mouse_event(&mut self, event: MouseEvent) {
        match event {
            MouseEvent::Move { x, y }
            | MouseEvent::ButtonDown { x, y, .. }
            | MouseEvent::ButtonUp { x, y, .. } => {
                self.mouse_x = x;
                self.mouse_y = y;
            }
            MouseEvent::Scroll { .. } => {}
        }
    }

    pub fn send_keyboard_event(&mut self, _event: pentimento_ipc::KeyboardEvent) {
        // Future: handle keyboard input
    }
}

// ============================================================================
// Render World Resources
// ============================================================================

/// Thread-safe Vello renderer stored in the render world.
#[derive(Resource)]
pub struct RenderWorldVelloRenderer {
    pub renderer: SharedVelloRenderer,
}

/// Pre-built Vello scene for the current frame.
#[derive(Resource, Default)]
pub struct VelloSceneBuffer {
    pub scene: Scene,
}

/// Track initialization status in render world.
#[derive(Resource, Default)]
pub struct VelloRenderStatus {
    pub first_render_done: bool,
}

// ============================================================================
// Plugin
// ============================================================================

/// Plugin for Dioxus UI rendering with zero-copy GPU integration.
pub struct DioxusRenderPlugin;

impl Plugin for DioxusRenderPlugin {
    fn build(&self, app: &mut App) {
        // Main world setup
        app.init_resource::<DioxusUiState>()
            .init_resource::<DioxusInputState>()
            .add_plugins(ExtractResourcePlugin::<DioxusUiState>::default())
            .add_plugins(ExtractResourcePlugin::<DioxusRenderTargetId>::default())
            .add_systems(Startup, setup_dioxus_texture)
            .add_systems(Update, (update_ui_state, handle_window_resize));

        // Render world setup
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("DioxusRenderPlugin: RenderApp not available, skipping render world setup");
            return;
        };

        render_app
            .init_resource::<VelloSceneBuffer>()
            .init_resource::<VelloRenderStatus>()
            .add_systems(
                Render,
                prepare_vello_scene
                    .in_set(RenderSystems::Prepare)
                    .before(render_vello_to_texture),
            )
            .add_systems(Render, render_vello_to_texture.in_set(RenderSystems::Render));
    }

    fn finish(&self, app: &mut App) {
        // Initialize Vello renderer AFTER RenderDevice is available
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        let render_device = render_app.world().resource::<RenderDevice>();
        match SharedVelloRenderer::new(render_device.wgpu_device()) {
            Ok(renderer) => {
                render_app.insert_resource(RenderWorldVelloRenderer { renderer });
                info!("Vello renderer initialized in render world (zero-copy mode)");
            }
            Err(e) => {
                error!("Failed to create Vello renderer in render world: {}", e);
            }
        }
    }
}

// ============================================================================
// Main World Systems
// ============================================================================

/// Initialize the Dioxus UI texture and overlay node.
/// This is an exclusive system because DioxusBridgeResource is NonSend.
fn setup_dioxus_texture(world: &mut World) {
    // Get window dimensions
    let (width, height) = {
        let mut window_query = world.query::<&Window>();
        let Some(window) = window_query.iter(world).next() else {
            error!("No window found for Dioxus UI setup");
            return;
        };
        (
            window.resolution.physical_width(),
            window.resolution.physical_height(),
        )
    };

    info!(
        "Setting up Dioxus UI texture ({}x{} physical, zero-copy mode)",
        width, height
    );

    // Create the IPC bridge (non-send due to mpsc::Receiver)
    let (_bridge, bridge_handle) = DioxusBridge::new();
    world.insert_non_send_resource(DioxusBridgeResource { bridge_handle });

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
    world.insert_resource(DioxusUiState {
        width,
        height,
        ..default()
    });

    // Create a full-screen UI node with the texture
    world.spawn((
        ImageNode {
            image: handle,
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

/// Update UI state from game state (runs every frame in main world).
fn update_ui_state(mut ui_state: ResMut<DioxusUiState>, time: Res<Time>) {
    // Update performance stats
    ui_state.fps = 1.0 / time.delta_secs();
    ui_state.frame_time = time.delta_secs() * 1000.0;
}

/// Handle window resize - update texture and UI state.
fn handle_window_resize(
    mut ui_state: ResMut<DioxusUiState>,
    render_target: Option<Res<DioxusRenderTarget>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, Changed<Window>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Some(render_target) = render_target else {
        return;
    };

    let width = window.resolution.physical_width();
    let height = window.resolution.physical_height();

    if width == ui_state.width && height == ui_state.height {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!(
        "Window resized to {}x{} physical, updating UI texture",
        width, height
    );

    // Update state (will be extracted to render world)
    ui_state.width = width;
    ui_state.height = height;

    // Resize the Bevy Image asset
    if let Some(image) = images.get_mut(&render_target.handle) {
        image.resize(Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        });
    }
}

// ============================================================================
// Render World Systems
// ============================================================================

/// Build the Vello scene from extracted UI state (runs in Prepare set).
fn prepare_vello_scene(
    ui_state: Option<Res<DioxusUiState>>,
    mut scene_buffer: ResMut<VelloSceneBuffer>,
) {
    let Some(ui_state) = ui_state else {
        return;
    };

    scene_buffer.scene.reset();

    // Build UI elements
    draw_toolbar(&mut scene_buffer.scene, ui_state.width as f64);
    draw_side_panel(
        &mut scene_buffer.scene,
        ui_state.width as f64,
        ui_state.height as f64,
    );
}

/// Render Vello scene directly to Bevy's GPU texture (runs in Render set).
fn render_vello_to_texture(
    ui_state: Option<Res<DioxusUiState>>,
    render_target: Option<Res<DioxusRenderTargetId>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    vello: Option<Res<RenderWorldVelloRenderer>>,
    scene: Res<VelloSceneBuffer>,
    mut status: ResMut<VelloRenderStatus>,
) {
    let Some(ui_state) = ui_state else {
        return;
    };

    let Some(render_target) = render_target else {
        return;
    };

    let Some(vello) = vello else {
        return;
    };

    // Get the GpuImage (Bevy's GPU-side texture representation)
    let Some(gpu_image) = gpu_images.get(render_target.0) else {
        // Texture not yet prepared - this is normal on the first few frames
        return;
    };

    // Zero-copy: render directly to Bevy's texture!
    if let Err(e) = vello.renderer.render_to_texture(
        render_device.wgpu_device(),
        render_queue.0.as_ref(),
        &scene.scene,
        &gpu_image.texture_view,
        &RenderParams {
            base_color: peniko::Color::TRANSPARENT,
            width: ui_state.width,
            height: ui_state.height,
            antialiasing_method: AaConfig::Area,
        },
    ) {
        error!("Vello render failed: {}", e);
        return;
    }

    if !status.first_render_done {
        info!("First Vello render completed (zero-copy to GpuImage)");
        status.first_render_done = true;
    }
}

// ============================================================================
// Scene Building Helpers
// ============================================================================

/// Draw the toolbar at the top of the screen.
fn draw_toolbar(scene: &mut Scene, width: f64) {
    use kurbo::{Affine, RoundedRect};
    use peniko::{Brush, Color, Fill};

    let toolbar_height = 48.0;

    // Background - semi-transparent dark gray (85% opacity = 216/255)
    let bg_color = Color::from_rgba8(30, 30, 30, 216);
    let rect = RoundedRect::from_rect(kurbo::Rect::new(0.0, 0.0, width, toolbar_height), 0.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(bg_color),
        None,
        &rect,
    );

    // Title placeholder - white rectangle
    let title_color = Color::WHITE;
    let title_rect = RoundedRect::from_rect(kurbo::Rect::new(16.0, 14.0, 96.0, 34.0), 2.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(title_color),
        None,
        &title_rect,
    );

    // Bottom border (10% opacity = 26/255)
    let border_color = Color::from_rgba8(255, 255, 255, 26);
    let border_rect = RoundedRect::from_rect(
        kurbo::Rect::new(0.0, toolbar_height - 1.0, width, toolbar_height),
        0.0,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(border_color),
        None,
        &border_rect,
    );
}

/// Draw the side panel on the right.
fn draw_side_panel(scene: &mut Scene, width: f64, height: f64) {
    use kurbo::{Affine, RoundedRect};
    use peniko::{Brush, Color, Fill};

    let panel_width = 300.0;
    let panel_top = 56.0;
    let panel_margin = 8.0;

    // Panel background (85% opacity)
    let bg_color = Color::from_rgba8(30, 30, 30, 216);
    let rect = RoundedRect::from_rect(
        kurbo::Rect::new(
            width - panel_width - panel_margin,
            panel_top,
            width - panel_margin,
            height - panel_margin,
        ),
        6.0,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(bg_color),
        None,
        &rect,
    );
}

// ============================================================================
// Legacy Compatibility - Resource for input module
// ============================================================================

/// Compatibility wrapper for the input module.
/// The input module expects DioxusRendererResource with send_mouse_event/send_keyboard_event.
pub struct DioxusRendererResource;

impl DioxusRendererResource {
    pub fn new() -> Self {
        Self
    }

    pub fn send_mouse_event(&mut self, _event: MouseEvent) {
        // Input handling is now done via DioxusInputState resource
    }

    pub fn send_keyboard_event(&mut self, _event: pentimento_ipc::KeyboardEvent) {
        // Input handling is now done via DioxusInputState resource
    }
}
