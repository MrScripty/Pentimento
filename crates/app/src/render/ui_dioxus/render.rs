//! Vello rendering to GPU texture in the render world.

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use pentimento_dioxus_ui::{peniko, AaConfig, RenderParams, SharedVelloRenderer};

use super::resources::{DioxusRenderTargetId, DioxusUiState, VelloRenderStatus, VelloSceneBuffer};

/// Thread-safe Vello renderer stored in the render world.
#[derive(Resource)]
pub struct RenderWorldVelloRenderer {
    pub renderer: SharedVelloRenderer,
}

/// Render Vello scene directly to Bevy's GPU texture (runs in Render set).
pub fn render_vello_to_texture(
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
