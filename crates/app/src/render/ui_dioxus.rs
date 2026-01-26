//! Dioxus UI Compositing - Zero-Copy GPU rendering with Vello via Blitz
//!
//! This module renders the Dioxus UI using Blitz for DOM/CSS/layout and Vello
//! for GPU rendering directly to a Bevy-owned texture, eliminating CPU-side copies.
//!
//! # Architecture
//!
//! 1. Main world: BlitzDocument manages Dioxus VirtualDom + Blitz DOM/layout
//! 2. Main world (Update): poll() processes state changes, paint_to_scene() builds Vello scene
//! 3. Extraction: Scene is cloned to render world
//! 4. Render world: Vello renders the scene directly to Bevy's GpuImage
//! 5. Bevy composites the texture over the 3D scene
//!
//! # Thread Safety
//!
//! BlitzDocument contains !Send types (VirtualDom), so it stays in main world.
//! Only the Scene (which is Clone+Send) is extracted to the render world.
//! Vello's Renderer is wrapped in `Arc<Mutex<...>>` for thread safety.

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
    peniko, AaConfig, BlitzDocument, BlitzKey, BlitzKeyCode, BlitzKeyEvent, BlitzKeyLocation,
    BlitzModifiers, BlitzPointerId, BlitzPointerEvent, BlitzWheelDelta, BlitzWheelEvent,
    DioxusBridge, DioxusBridgeHandle, KeyState, MouseEventButton, MouseEventButtons, PointerCoords,
    PointerDetails, RenderParams, Scene, SharedVelloRenderer, UiEvent,
};
use pentimento_ipc::{MouseEvent, PaintCommand, UiToBevy};
use pentimento_scene::{CanvasPlaneEvent, OutboundUiMessages, PaintingResource};

use super::ui_blend_material::{UiBlendMaterial, UiBlendMaterialPlugin};

// ============================================================================
// Main World Resources
// ============================================================================

/// UI state that gets extracted to the render world each frame.
/// This contains viewport dimensions needed for Vello rendering.
#[derive(Resource, Clone, ExtractResource, Default)]
pub struct DioxusUiState {
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

/// The BlitzDocument that manages the Dioxus VirtualDom and Blitz DOM/layout.
/// This is a NonSend resource because VirtualDom is !Send.
pub struct BlitzDocumentResource {
    pub document: BlitzDocument,
}

/// Pre-built Vello scene for the current frame.
/// Built in main world, extracted to render world.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct VelloSceneBuffer {
    pub scene: Scene,
}

/// Input state for the Dioxus UI (main world).
/// Queues UI events to be processed in the build_ui_scene system.
#[derive(Resource, Default)]
pub struct DioxusInputState {
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// Track which mouse buttons are currently pressed
    pub buttons_pressed: MouseEventButtons,
    /// Queued UI events to be processed
    pub event_queue: Vec<UiEvent>,
}

impl DioxusInputState {
    pub fn send_mouse_event(&mut self, event: MouseEvent) {
        match event {
            MouseEvent::Move { x, y } => {
                self.mouse_x = x;
                self.mouse_y = y;
                self.event_queue.push(UiEvent::PointerMove(self.create_pointer_event(x, y, MouseEventButton::Main)));
            }
            MouseEvent::ButtonDown { x, y, button } => {
                self.mouse_x = x;
                self.mouse_y = y;
                let btn = self.convert_button(button);
                self.buttons_pressed.insert(MouseEventButtons::from(btn));
                self.event_queue.push(UiEvent::PointerDown(self.create_pointer_event(x, y, btn)));
            }
            MouseEvent::ButtonUp { x, y, button } => {
                self.mouse_x = x;
                self.mouse_y = y;
                let btn = self.convert_button(button);
                self.buttons_pressed.remove(MouseEventButtons::from(btn));
                self.event_queue.push(UiEvent::PointerUp(self.create_pointer_event(x, y, btn)));
            }
            MouseEvent::Scroll { delta_x, delta_y, x, y } => {
                self.mouse_x = x;
                self.mouse_y = y;
                self.event_queue.push(UiEvent::Wheel(BlitzWheelEvent {
                    delta: BlitzWheelDelta::Pixels(delta_x as f64, delta_y as f64),
                    coords: PointerCoords {
                        page_x: x,
                        page_y: y,
                        screen_x: x,
                        screen_y: y,
                        client_x: x,
                        client_y: y,
                    },
                    buttons: self.buttons_pressed,
                    mods: BlitzModifiers::empty(),
                }));
            }
        }
    }

    pub fn send_keyboard_event(&mut self, event: pentimento_ipc::KeyboardEvent) {
        // Convert IPC modifiers to Blitz modifiers
        let mut mods = BlitzModifiers::empty();
        if event.modifiers.shift {
            mods.insert(BlitzModifiers::SHIFT);
        }
        if event.modifiers.ctrl {
            mods.insert(BlitzModifiers::CONTROL);
        }
        if event.modifiers.alt {
            mods.insert(BlitzModifiers::ALT);
        }
        if event.modifiers.meta {
            mods.insert(BlitzModifiers::META);
        }

        // Convert key string to Blitz Key type
        let key = self.convert_key(&event.key);
        let code = self.convert_code(&event.key);

        let key_event = BlitzKeyEvent {
            key,
            code,
            modifiers: mods,
            location: BlitzKeyLocation::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: if event.pressed { KeyState::Pressed } else { KeyState::Released },
            text: if event.pressed && event.key.len() == 1 {
                Some(event.key.clone().into())
            } else {
                None
            },
        };

        let ui_event = if event.pressed {
            UiEvent::KeyDown(key_event)
        } else {
            UiEvent::KeyUp(key_event)
        };

        self.event_queue.push(ui_event);
    }

    fn convert_key(&self, key_str: &str) -> BlitzKey {
        match key_str {
            "Enter" => BlitzKey::Enter,
            "Escape" => BlitzKey::Escape,
            "Backspace" => BlitzKey::Backspace,
            "Tab" => BlitzKey::Tab,
            "Delete" => BlitzKey::Delete,
            "ArrowUp" => BlitzKey::ArrowUp,
            "ArrowDown" => BlitzKey::ArrowDown,
            "ArrowLeft" => BlitzKey::ArrowLeft,
            "ArrowRight" => BlitzKey::ArrowRight,
            "Home" => BlitzKey::Home,
            "End" => BlitzKey::End,
            "PageUp" => BlitzKey::PageUp,
            "PageDown" => BlitzKey::PageDown,
            "Shift" | "Control" | "Alt" | "Meta" => BlitzKey::Unidentified,
            k => BlitzKey::Character(k.into()),
        }
    }

    fn convert_code(&self, key_str: &str) -> BlitzKeyCode {
        match key_str.to_lowercase().as_str() {
            "a" => BlitzKeyCode::KeyA,
            "b" => BlitzKeyCode::KeyB,
            "c" => BlitzKeyCode::KeyC,
            "d" => BlitzKeyCode::KeyD,
            "e" => BlitzKeyCode::KeyE,
            "f" => BlitzKeyCode::KeyF,
            "g" => BlitzKeyCode::KeyG,
            "h" => BlitzKeyCode::KeyH,
            "i" => BlitzKeyCode::KeyI,
            "j" => BlitzKeyCode::KeyJ,
            "k" => BlitzKeyCode::KeyK,
            "l" => BlitzKeyCode::KeyL,
            "m" => BlitzKeyCode::KeyM,
            "n" => BlitzKeyCode::KeyN,
            "o" => BlitzKeyCode::KeyO,
            "p" => BlitzKeyCode::KeyP,
            "q" => BlitzKeyCode::KeyQ,
            "r" => BlitzKeyCode::KeyR,
            "s" => BlitzKeyCode::KeyS,
            "t" => BlitzKeyCode::KeyT,
            "u" => BlitzKeyCode::KeyU,
            "v" => BlitzKeyCode::KeyV,
            "w" => BlitzKeyCode::KeyW,
            "x" => BlitzKeyCode::KeyX,
            "y" => BlitzKeyCode::KeyY,
            "z" => BlitzKeyCode::KeyZ,
            "0" => BlitzKeyCode::Digit0,
            "1" => BlitzKeyCode::Digit1,
            "2" => BlitzKeyCode::Digit2,
            "3" => BlitzKeyCode::Digit3,
            "4" => BlitzKeyCode::Digit4,
            "5" => BlitzKeyCode::Digit5,
            "6" => BlitzKeyCode::Digit6,
            "7" => BlitzKeyCode::Digit7,
            "8" => BlitzKeyCode::Digit8,
            "9" => BlitzKeyCode::Digit9,
            " " => BlitzKeyCode::Space,
            "enter" => BlitzKeyCode::Enter,
            "escape" => BlitzKeyCode::Escape,
            "backspace" => BlitzKeyCode::Backspace,
            "tab" => BlitzKeyCode::Tab,
            _ => BlitzKeyCode::Unidentified,
        }
    }

    fn convert_button(&self, button: pentimento_ipc::MouseButton) -> MouseEventButton {
        match button {
            pentimento_ipc::MouseButton::Left => MouseEventButton::Main,
            pentimento_ipc::MouseButton::Right => MouseEventButton::Secondary,
            pentimento_ipc::MouseButton::Middle => MouseEventButton::Auxiliary,
        }
    }

    fn create_pointer_event(&self, x: f32, y: f32, button: MouseEventButton) -> BlitzPointerEvent {
        BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,
                screen_x: x,
                screen_y: y,
                client_x: x,
                client_y: y,
            },
            button,
            buttons: self.buttons_pressed,
            mods: BlitzModifiers::empty(),
            details: PointerDetails::default(),
        }
    }
}

// ============================================================================
// Dioxus Channel-Based Renderer Resources
// ============================================================================

/// Event sender for channel-based Dioxus UI event handling.
#[derive(Clone)]
pub struct DioxusEventSender(pub std::sync::mpsc::Sender<UiEvent>);

/// Resource for sending events to the Dioxus UI thread via channel.
/// Uses click tolerance to prevent small mouse movement from triggering drag detection.
#[derive(Resource)]
pub struct DioxusRendererResource {
    sender: DioxusEventSender,
    mouse_x: f32,
    mouse_y: f32,
    buttons_pressed: MouseEventButtons,
    /// Position where the mouse button was pressed (for click vs drag detection)
    mousedown_x: f32,
    mousedown_y: f32,
}

/// Click tolerance in logical pixels. Movement within this distance from mousedown
/// won't trigger drag mode, making clicks more reliable on sensitive input devices.
const CLICK_TOLERANCE: f32 = 8.0;

impl DioxusRendererResource {
    pub fn new(sender: DioxusEventSender) -> Self {
        Self {
            sender,
            mouse_x: 0.0,
            mouse_y: 0.0,
            buttons_pressed: MouseEventButtons::empty(),
            mousedown_x: 0.0,
            mousedown_y: 0.0,
        }
    }

    pub fn send_mouse_event(&mut self, event: MouseEvent) {
        let ui_event = match event {
            MouseEvent::Move { x, y } => {
                self.mouse_x = x;
                self.mouse_y = y;
                // Only report buttons as pressed if we've moved beyond click tolerance.
                // This prevents small mouse jitter from triggering Blitz's drag detection
                // (which would prevent the click from being synthesized).
                let buttons = if self.buttons_pressed.is_empty() {
                    MouseEventButtons::empty()
                } else {
                    let dx = (x - self.mousedown_x).abs();
                    let dy = (y - self.mousedown_y).abs();
                    if dx > CLICK_TOLERANCE || dy > CLICK_TOLERANCE {
                        self.buttons_pressed
                    } else {
                        MouseEventButtons::empty()
                    }
                };
                UiEvent::PointerMove(self.create_pointer_event_with_buttons(
                    x,
                    y,
                    MouseEventButton::Main,
                    buttons,
                ))
            }
            MouseEvent::ButtonDown { x, y, button } => {
                self.mouse_x = x;
                self.mouse_y = y;
                self.mousedown_x = x;
                self.mousedown_y = y;
                let btn = self.convert_button(button);
                self.buttons_pressed.insert(MouseEventButtons::from(btn));
                UiEvent::PointerDown(self.create_pointer_event(x, y, btn))
            }
            MouseEvent::ButtonUp { x, y, button } => {
                self.mouse_x = x;
                self.mouse_y = y;
                let btn = self.convert_button(button);
                self.buttons_pressed.remove(MouseEventButtons::from(btn));
                UiEvent::PointerUp(self.create_pointer_event(x, y, btn))
            }
            MouseEvent::Scroll { delta_x, delta_y, x, y } => {
                self.mouse_x = x;
                self.mouse_y = y;
                UiEvent::Wheel(BlitzWheelEvent {
                    delta: BlitzWheelDelta::Pixels(delta_x as f64, delta_y as f64),
                    coords: PointerCoords {
                        page_x: x,
                        page_y: y,
                        screen_x: x,
                        screen_y: y,
                        client_x: x,
                        client_y: y,
                    },
                    buttons: self.buttons_pressed,
                    mods: BlitzModifiers::empty(),
                })
            }
        };

        // Send through channel (ignore errors if receiver is dropped)
        if let Err(e) = self.sender.0.send(ui_event) {
            error!("Failed to send UI event through channel: {}", e);
        }
    }

    pub fn send_keyboard_event(&mut self, event: pentimento_ipc::KeyboardEvent) {
        // Convert IPC modifiers to Blitz modifiers
        let mut mods = BlitzModifiers::empty();
        if event.modifiers.shift {
            mods.insert(BlitzModifiers::SHIFT);
        }
        if event.modifiers.ctrl {
            mods.insert(BlitzModifiers::CONTROL);
        }
        if event.modifiers.alt {
            mods.insert(BlitzModifiers::ALT);
        }
        if event.modifiers.meta {
            mods.insert(BlitzModifiers::META);
        }

        // Convert key string to Blitz Key type
        let key = self.convert_key(&event.key);
        let code = self.convert_code(&event.key);

        let key_event = BlitzKeyEvent {
            key,
            code,
            modifiers: mods,
            location: BlitzKeyLocation::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: if event.pressed { KeyState::Pressed } else { KeyState::Released },
            text: if event.pressed && event.key.len() == 1 {
                Some(event.key.clone().into())
            } else {
                None
            },
        };

        let ui_event = if event.pressed {
            UiEvent::KeyDown(key_event)
        } else {
            UiEvent::KeyUp(key_event)
        };

        // Send through channel (ignore errors if receiver is dropped)
        if let Err(e) = self.sender.0.send(ui_event) {
            error!("Failed to send keyboard event through channel: {}", e);
        }
    }

    fn convert_key(&self, key_str: &str) -> BlitzKey {
        match key_str {
            "Enter" => BlitzKey::Enter,
            "Escape" => BlitzKey::Escape,
            "Backspace" => BlitzKey::Backspace,
            "Tab" => BlitzKey::Tab,
            "Delete" => BlitzKey::Delete,
            "ArrowUp" => BlitzKey::ArrowUp,
            "ArrowDown" => BlitzKey::ArrowDown,
            "ArrowLeft" => BlitzKey::ArrowLeft,
            "ArrowRight" => BlitzKey::ArrowRight,
            "Home" => BlitzKey::Home,
            "End" => BlitzKey::End,
            "PageUp" => BlitzKey::PageUp,
            "PageDown" => BlitzKey::PageDown,
            "Shift" | "Control" | "Alt" | "Meta" => BlitzKey::Unidentified,
            k => BlitzKey::Character(k.into()),
        }
    }

    fn convert_code(&self, key_str: &str) -> BlitzKeyCode {
        match key_str.to_lowercase().as_str() {
            "a" => BlitzKeyCode::KeyA,
            "b" => BlitzKeyCode::KeyB,
            "c" => BlitzKeyCode::KeyC,
            "d" => BlitzKeyCode::KeyD,
            "e" => BlitzKeyCode::KeyE,
            "f" => BlitzKeyCode::KeyF,
            "g" => BlitzKeyCode::KeyG,
            "h" => BlitzKeyCode::KeyH,
            "i" => BlitzKeyCode::KeyI,
            "j" => BlitzKeyCode::KeyJ,
            "k" => BlitzKeyCode::KeyK,
            "l" => BlitzKeyCode::KeyL,
            "m" => BlitzKeyCode::KeyM,
            "n" => BlitzKeyCode::KeyN,
            "o" => BlitzKeyCode::KeyO,
            "p" => BlitzKeyCode::KeyP,
            "q" => BlitzKeyCode::KeyQ,
            "r" => BlitzKeyCode::KeyR,
            "s" => BlitzKeyCode::KeyS,
            "t" => BlitzKeyCode::KeyT,
            "u" => BlitzKeyCode::KeyU,
            "v" => BlitzKeyCode::KeyV,
            "w" => BlitzKeyCode::KeyW,
            "x" => BlitzKeyCode::KeyX,
            "y" => BlitzKeyCode::KeyY,
            "z" => BlitzKeyCode::KeyZ,
            "0" => BlitzKeyCode::Digit0,
            "1" => BlitzKeyCode::Digit1,
            "2" => BlitzKeyCode::Digit2,
            "3" => BlitzKeyCode::Digit3,
            "4" => BlitzKeyCode::Digit4,
            "5" => BlitzKeyCode::Digit5,
            "6" => BlitzKeyCode::Digit6,
            "7" => BlitzKeyCode::Digit7,
            "8" => BlitzKeyCode::Digit8,
            "9" => BlitzKeyCode::Digit9,
            " " => BlitzKeyCode::Space,
            "enter" => BlitzKeyCode::Enter,
            "escape" => BlitzKeyCode::Escape,
            "backspace" => BlitzKeyCode::Backspace,
            "tab" => BlitzKeyCode::Tab,
            _ => BlitzKeyCode::Unidentified,
        }
    }

    fn convert_button(&self, button: pentimento_ipc::MouseButton) -> MouseEventButton {
        match button {
            pentimento_ipc::MouseButton::Left => MouseEventButton::Main,
            pentimento_ipc::MouseButton::Right => MouseEventButton::Secondary,
            pentimento_ipc::MouseButton::Middle => MouseEventButton::Auxiliary,
        }
    }

    fn create_pointer_event(&self, x: f32, y: f32, button: MouseEventButton) -> BlitzPointerEvent {
        self.create_pointer_event_with_buttons(x, y, button, self.buttons_pressed)
    }

    fn create_pointer_event_with_buttons(
        &self,
        x: f32,
        y: f32,
        button: MouseEventButton,
        buttons: MouseEventButtons,
    ) -> BlitzPointerEvent {
        BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,
                screen_x: x,
                screen_y: y,
                client_x: x,
                client_y: y,
            },
            button,
            buttons,
            mods: BlitzModifiers::empty(),
            details: PointerDetails::default(),
        }
    }
}

/// Channel receiver for UI events, processed in build_ui_scene.
pub struct DioxusEventReceiver(pub std::sync::mpsc::Receiver<UiEvent>);

/// Create a new event channel pair for UI input events.
pub fn create_event_channel() -> (DioxusEventSender, DioxusEventReceiver) {
    let (tx, rx) = std::sync::mpsc::channel();
    (DioxusEventSender(tx), DioxusEventReceiver(rx))
}

// ============================================================================
// Render World Resources
// ============================================================================

/// Thread-safe Vello renderer stored in the render world.
#[derive(Resource)]
pub struct RenderWorldVelloRenderer {
    pub renderer: SharedVelloRenderer,
}

/// Track initialization status in render world.
#[derive(Resource, Default)]
pub struct VelloRenderStatus {
    pub first_render_done: bool,
}

/// Track whether Dioxus UI setup is complete (main world).
/// We defer setup to allow the window size to stabilize after creation.
#[derive(Resource, Default)]
pub struct DioxusSetupStatus {
    pub setup_done: bool,
    pub frames_waited: u32,
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
            .init_resource::<VelloSceneBuffer>()
            .init_resource::<DioxusSetupStatus>()
            .add_plugins(UiBlendMaterialPlugin)
            .add_plugins(ExtractResourcePlugin::<DioxusUiState>::default())
            .add_plugins(ExtractResourcePlugin::<DioxusRenderTargetId>::default())
            .add_plugins(ExtractResourcePlugin::<VelloSceneBuffer>::default())
            // Run setup during Update (not Startup) to allow window size to stabilize
            .add_systems(Update, (deferred_setup_dioxus_texture, build_ui_scene, handle_window_resize).chain())
            // Handle IPC messages from UI (runs after setup so bridge exists)
            .add_systems(Update, handle_ui_to_bevy_messages.after(deferred_setup_dioxus_texture));

        // Render world setup
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("DioxusRenderPlugin: RenderApp not available, skipping render world setup");
            return;
        };

        render_app
            .init_resource::<VelloRenderStatus>()
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

/// Deferred setup that waits for window size to stabilize before initializing.
/// This prevents issues where the window starts at one size and immediately resizes.
fn deferred_setup_dioxus_texture(world: &mut World) {
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
fn setup_dioxus_texture(world: &mut World) {
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

/// Build the UI scene from BlitzDocument (runs every frame in main world).
/// This is an exclusive system because BlitzDocumentResource is NonSend.
fn build_ui_scene(world: &mut World) {
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
            warn!("No DioxusEventReceiver found!");
            Vec::new()
        }
    };

    // Process input events and poll the document (needs mutable access, separate scope)
    let viewport_clicked = {
        let Some(mut doc_resource) = world.get_non_send_resource_mut::<BlitzDocumentResource>()
        else {
            return;
        };

        // Forward queued events to BlitzDocument
        for event in &events {
            doc_resource.document.handle_event(event.clone());
        }

        // Only force render when there were input events to process
        if !events.is_empty() {
            doc_resource.document.force_render();
        }

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
fn handle_window_resize(world: &mut World) {
    // Check if window changed
    // Use LOGICAL dimensions to match initial setup and mouse coordinates
    let (width, height, changed) = {
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

/// Handle IPC messages from the Dioxus UI and dispatch to appropriate Bevy events.
/// This is an exclusive system because DioxusBridgeResource is NonSend.
fn handle_ui_to_bevy_messages(world: &mut World) {
    use bevy::ecs::message::Messages;

    // Forward outbound messages (Bevy->UI) first
    let outbound_msgs = {
        if let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() {
            outbound.drain()
        } else {
            Vec::new()
        }
    };

    if !outbound_msgs.is_empty() {
        info!("Forwarding {} outbound messages to UI bridge", outbound_msgs.len());
        if let Some(bridge) = world.get_non_send_resource::<DioxusBridgeResource>() {
            for msg in outbound_msgs {
                info!("  -> Processing message: {:?}", msg);
                bridge.bridge_handle.send(msg);
            }
        } else {
            warn!("No DioxusBridgeResource found!");
        }

        // Force VirtualDom to render so component polls bridge and processes messages.
        // Normal poll() returns early if no signals changed, but channel messages
        // don't trigger signals - the component needs to render first.
        if let Some(mut doc_resource) = world.get_non_send_resource_mut::<BlitzDocumentResource>() {
            info!("Calling force_render() after forwarding messages");
            doc_resource.document.force_render();
        } else {
            warn!("No BlitzDocumentResource found!");
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
                    }
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

// ============================================================================
// Render World Systems
// ============================================================================

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

    // Log dimensions on first render to help diagnose fuzzy/sharp alternation
    if !status.first_render_done {
        let tex_size = gpu_image.size;
        info!(
            "First Vello render: ui_state={}x{}, texture={}x{}",
            ui_state.width, ui_state.height, tex_size.width, tex_size.height
        );
    }

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

