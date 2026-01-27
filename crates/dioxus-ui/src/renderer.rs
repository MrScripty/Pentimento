//! Vello-based GPU renderer for the Dioxus UI
//!
//! This module provides a renderer that uses Vello to draw the UI directly
//! to a GPU texture that can be shared with Bevy for zero-copy compositing.
//!
//! # Architecture
//!
//! The renderer accepts an external wgpu Device/Queue from Bevy, allowing
//! both Bevy and the UI to share the same GPU context. Vello renders to
//! a shared texture which Bevy then samples in its shader pipeline.
//!
//! # Thread Safety
//!
//! `SharedVelloRenderer` wraps Vello's Renderer in `Arc<Mutex<...>>` to make
//! it Send+Sync for use in Bevy's render world.

use std::sync::{Arc, Mutex};
use tracing::{debug, info};
use vello::kurbo::{Affine, RoundedRect};
use vello::peniko::{Brush, Color, Fill};
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

/// Vello-based renderer that draws UI to a GPU texture
///
/// This renderer uses Vello's GPU compute pipeline to render 2D graphics.
/// It can either create its own wgpu context or use one provided by Bevy.
pub struct VelloRenderer {
    renderer: Renderer,
    scene: Scene,
    width: u32,
    height: u32,
    dirty: bool,
}

impl VelloRenderer {
    /// Create a new Vello renderer using an external wgpu device
    ///
    /// This is the preferred method for Bevy integration as it allows
    /// zero-copy texture sharing between Vello and Bevy.
    pub fn new_with_device(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<Self, VelloRendererError> {
        info!("Creating Vello renderer {}x{} with shared device", width, height);

        let renderer = Renderer::new(
            device,
            RendererOptions {
                // Use area antialiasing for best quality
                antialiasing_support: vello::AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| VelloRendererError::RendererCreation(format!("{:?}", e)))?;

        let scene = Scene::new();

        Ok(Self {
            renderer,
            scene,
            width,
            height,
            dirty: true,
        })
    }

    /// Resize the renderer
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.width != width || self.height != height {
            info!("Vello renderer resize: {}x{}", width, height);
            self.width = width;
            self.height = height;
            self.dirty = true;
        }
    }

    /// Mark the scene as needing re-render
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Check if the scene needs re-rendering
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get the current dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Build the UI scene
    ///
    /// This constructs the Vello scene graph for the current UI state.
    /// Called before rendering to update the scene.
    pub fn build_scene(&mut self, ui_state: &UiRenderState) {
        self.scene.reset();

        // Draw toolbar background
        self.draw_toolbar(ui_state);

        // Draw side panel
        self.draw_side_panel(ui_state);

        self.dirty = false;
    }

    /// Draw the toolbar at the top of the screen
    fn draw_toolbar(&mut self, _ui_state: &UiRenderState) {
        let toolbar_height = 48.0;
        let width = self.width as f64;

        // Background - semi-transparent dark gray (90% opacity = 230/255)
        let bg_color = Color::from_rgba8(30, 30, 30, 230);
        let rect = RoundedRect::from_rect(
            vello::kurbo::Rect::new(0.0, 0.0, width, toolbar_height),
            0.0,
        );
        self.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(bg_color),
            None,
            &rect,
        );

        // Title placeholder - white rectangle
        let title_color = Color::WHITE;
        let title_rect = RoundedRect::from_rect(
            vello::kurbo::Rect::new(16.0, 14.0, 96.0, 34.0),
            2.0,
        );
        self.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(title_color),
            None,
            &title_rect,
        );

        // Bottom border (10% opacity = 26/255)
        let border_color = Color::from_rgba8(255, 255, 255, 26);
        let border_rect = RoundedRect::from_rect(
            vello::kurbo::Rect::new(0.0, toolbar_height - 1.0, width, toolbar_height),
            0.0,
        );
        self.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(border_color),
            None,
            &border_rect,
        );
    }

    /// Draw the side panel on the right
    fn draw_side_panel(&mut self, _ui_state: &UiRenderState) {
        let panel_width = 300.0;
        let panel_top = 56.0;
        let panel_margin = 8.0;
        let width = self.width as f64;
        let height = self.height as f64;

        // Panel background (90% opacity)
        let bg_color = Color::from_rgba8(30, 30, 30, 230);
        let rect = RoundedRect::from_rect(
            vello::kurbo::Rect::new(
                width - panel_width - panel_margin,
                panel_top,
                width - panel_margin,
                height - panel_margin,
            ),
            6.0,
        );
        self.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(bg_color),
            None,
            &rect,
        );
    }

    /// Render the scene to a texture
    ///
    /// This is the core rendering function that uses Vello's GPU compute
    /// pipeline to render the UI scene to the provided texture view.
    pub fn render_to_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_view: &wgpu::TextureView,
    ) -> Result<(), VelloRendererError> {
        debug!("Vello render_to_texture {}x{}", self.width, self.height);

        self.renderer
            .render_to_texture(
                device,
                queue,
                &self.scene,
                texture_view,
                &RenderParams {
                    base_color: Color::TRANSPARENT,
                    width: self.width,
                    height: self.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| VelloRendererError::RenderFailed(format!("{:?}", e)))?;

        Ok(())
    }
}

/// UI state for rendering
///
/// This holds the current state of the UI that needs to be rendered.
/// It's a simplified view of the full UI state for the renderer.
#[derive(Debug, Clone, Default)]
pub struct UiRenderState {
    /// FPS display value
    pub fps: f32,
    /// Frame time in milliseconds
    pub frame_time: f32,
    /// Currently selected tool
    pub selected_tool: String,
    /// Open menu (if any)
    pub open_menu: Option<String>,
    /// Selected object IDs
    pub selected_objects: Vec<String>,
}

/// Errors that can occur during Vello rendering
#[derive(Debug)]
pub enum VelloRendererError {
    /// Failed to create Vello renderer
    RendererCreation(String),
    /// Failed to render scene
    RenderFailed(String),
    /// Texture creation failed
    TextureCreation(String),
}

impl std::fmt::Display for VelloRendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RendererCreation(msg) => write!(f, "Failed to create Vello renderer: {}", msg),
            Self::RenderFailed(msg) => write!(f, "Vello render failed: {}", msg),
            Self::TextureCreation(msg) => write!(f, "Texture creation failed: {}", msg),
        }
    }
}

impl std::error::Error for VelloRendererError {}

/// Thread-safe Vello renderer wrapper for Bevy's render world.
///
/// Wraps Vello's `Renderer` in `Arc<Mutex<...>>` to be Send+Sync for use
/// across render threads. The mutex is only held during rendering, which
/// happens once per frame with no contention.
#[derive(Clone)]
pub struct SharedVelloRenderer(Arc<Mutex<Renderer>>);

impl SharedVelloRenderer {
    /// Create a new thread-safe Vello renderer.
    ///
    /// This should be called in Bevy's plugin `finish()` method after
    /// `RenderDevice` is available.
    pub fn new(device: &wgpu::Device) -> Result<Self, VelloRendererError> {
        info!("Creating SharedVelloRenderer for render world");
        let renderer = Renderer::new(
            device,
            RendererOptions {
                antialiasing_support: vello::AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| VelloRendererError::RendererCreation(format!("{:?}", e)))?;

        Ok(Self(Arc::new(Mutex::new(renderer))))
    }

    /// Render a scene directly to a texture view.
    ///
    /// This is the zero-copy path: Vello renders directly to Bevy's GpuImage
    /// texture without any CPU-side buffer copies.
    pub fn render_to_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        texture_view: &wgpu::TextureView,
        params: &RenderParams,
    ) -> Result<(), VelloRendererError> {
        debug!(
            "SharedVelloRenderer::render_to_texture {}x{}",
            params.width, params.height
        );

        self.0
            .lock()
            .unwrap()
            .render_to_texture(device, queue, scene, texture_view, params)
            .map_err(|e| VelloRendererError::RenderFailed(format!("{:?}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_render_state_default() {
        let state = UiRenderState::default();
        assert_eq!(state.fps, 0.0);
        assert_eq!(state.frame_time, 0.0);
        assert!(state.selected_tool.is_empty());
        assert!(state.open_menu.is_none());
        assert!(state.selected_objects.is_empty());
    }
}
