//! Edge detection post-process for Surface ID outline rendering
//!
//! This module implements a per-view render system that reads the ID buffer
//! and composites orange outlines onto the scene where entity IDs differ.
//! Uses the standard Bevy post-processing pattern with ViewTarget::post_process_write().

use bevy::asset::embedded_asset;
use bevy::core_pipeline::FullscreenShader;
use bevy::core_pipeline::{Core3dSystems, schedule::Core3d};
use bevy::prelude::*;
use bevy::render::{
    GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems,
    extract_component::ExtractComponentPlugin,
    extract_resource::ExtractResourcePlugin,
    render_asset::RenderAssets,
    render_resource::{
        BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
        BufferInitDescriptor, BufferUsages, CachedRenderPipelineId, ColorTargetState, ColorWrites,
        FragmentState, MultisampleState, Operations, PipelineCache, PrimitiveState,
        RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler,
        SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, TextureFormat,
        TextureSampleType,
        binding_types::{sampler, texture_2d, uniform_buffer},
    },
    renderer::{RenderContext, RenderDevice, ViewQuery},
    texture::GpuImage,
    view::ViewTarget,
};

use super::OutlineCamera;
use super::OutlineRenderTargets;
use super::outline_settings::OutlineSettings;

/// Plugin for edge detection post-processing
pub struct EdgeDetectionPlugin;

impl Plugin for EdgeDetectionPlugin {
    fn build(&self, app: &mut App) {
        // Embed the shader
        embedded_asset!(app, "shaders/edge_detection.wesl");

        // Extract OutlineCamera component to render world
        app.add_plugins(ExtractComponentPlugin::<OutlineCamera>::default());
        app.add_plugins(ExtractResourcePlugin::<OutlineSettings>::default());
        app.add_plugins(ExtractResourcePlugin::<OutlineRenderTargets>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            bevy::log::warn!("EdgeDetectionPlugin: No RenderApp available!");
            return;
        };

        // Prepared textures and buffers belong to the previous device on recovery.
        render_app.add_systems(RenderStartup, clear_prepare_edge_detection_resources);

        // Post-process the current view after tonemapping; outlines follow depth.
        render_app.add_systems(
            Core3d,
            render_edge_detection
                .in_set(Core3dSystems::PostProcess)
                .in_set(EdgeDetectionLabel)
                .after(crate::DepthViewLabel),
        );

        render_app.add_systems(
            Render,
            prepare_edge_detection.in_set(RenderSystems::PrepareBindGroups),
        );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            bevy::log::warn!("EdgeDetectionPlugin::finish: No RenderApp available!");
            return;
        };

        render_app.init_gpu_resource::<EdgeDetectionPipeline>();
        bevy::log::info!("EdgeDetectionPlugin: Pipeline initialized");
    }
}

/// Render schedule set for edge detection
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub struct EdgeDetectionLabel;

/// Uniform data for edge detection shader
#[derive(Clone, Copy, ShaderType)]
pub struct EdgeDetectionUniform {
    pub outline_color: Vec4,
    pub thickness: f32,
    pub texture_size: Vec2,
    pub _padding: f32,
}

/// Render the outline composite for the current main view.
fn render_edge_detection(
    view: ViewQuery<&ViewTarget, With<OutlineCamera>>,
    settings: Option<Res<OutlineSettings>>,
    prepared: Option<Res<EdgeDetectionPrepared>>,
    pipeline: Option<Res<EdgeDetectionPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
    mut render_context: RenderContext,
) {
    let (Some(settings), Some(prepared), Some(pipeline)) = (settings, prepared, pipeline) else {
        return;
    };
    if !settings.enabled {
        return;
    }
    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline.pipeline_id) else {
        return;
    };

    let post_process = view.into_inner().post_process_write();
    let bind_group = render_device.create_bind_group(
        "edge_detection_bind_group",
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            prepared.uniform_buffer.as_entire_binding(),
            &prepared.id_texture_view,
            &pipeline.sampler,
            post_process.source,
            &pipeline.sampler,
        )),
    );
    let mut render_pass =
        render_context
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("edge_detection_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: post_process.destination,
                    resolve_target: None,
                    ops: Operations::default(),
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
    render_pass.set_pipeline(render_pipeline);
    render_pass.set_bind_group(0, &bind_group, &[]);
    render_pass.draw(0..3, 0..1);
}

/// Pipeline for edge detection
#[derive(Resource)]
pub struct EdgeDetectionPipeline {
    pub layout: BindGroupLayoutDescriptor,
    pub sampler: Sampler,
    pub pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for EdgeDetectionPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        // Create bind group layout entries
        // Bindings: uniform, id_texture, id_sampler, scene_texture, scene_sampler
        let layout_entries = BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                uniform_buffer::<EdgeDetectionUniform>(false),
                texture_2d(TextureSampleType::Float { filterable: true }), // ID buffer
                sampler(SamplerBindingType::Filtering),                    // ID sampler
                texture_2d(TextureSampleType::Float { filterable: true }), // Scene texture
                sampler(SamplerBindingType::Filtering),                    // Scene sampler
            ),
        );

        // Create the descriptor for the pipeline
        let layout_descriptor = BindGroupLayoutDescriptor::new(
            "edge_detection_bind_group_layout",
            &layout_entries.to_vec(),
        );

        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        let shader =
            world.load_asset("embedded://pentimento_scene/outline/shaders/edge_detection.wesl");

        let fullscreen_shader = world.resource::<FullscreenShader>();
        let vertex_state = fullscreen_shader.to_vertex_state();

        let pipeline_id =
            world
                .resource_mut::<PipelineCache>()
                .queue_render_pipeline(RenderPipelineDescriptor {
                    label: Some("edge_detection_pipeline".into()),
                    layout: vec![layout_descriptor.clone()],
                    vertex: vertex_state,
                    fragment: Some(FragmentState {
                        constants: vec![],
                        shader,
                        shader_defs: vec![],
                        entry_point: Some("fragment".into()),
                        targets: vec![Some(ColorTargetState {
                            // Use HDR format to match ViewTarget (atmosphere enables HDR)
                            format: TextureFormat::Rgba16Float,
                            blend: None,
                            write_mask: ColorWrites::ALL,
                        })],
                    }),
                    primitive: PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: MultisampleState::default(),
                    immediate_size: 0,
                    zero_initialize_workgroup_memory: false,
                });

        Self {
            layout: layout_descriptor,
            sampler,
            pipeline_id,
        }
    }
}

/// Prepared data for edge detection (created during Prepare phase)
#[derive(Resource)]
pub struct EdgeDetectionPrepared {
    pub uniform_buffer: Buffer,
    pub id_texture_view: bevy::render::render_resource::TextureView,
}

/// Prepare the edge detection data each frame
fn prepare_edge_detection(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    settings: Option<Res<OutlineSettings>>,
    targets: Option<Res<OutlineRenderTargets>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
) {
    let Some(settings) = settings else {
        return;
    };
    let Some(targets) = targets else {
        return;
    };

    // Get the GPU texture for the ID buffer
    let Some(id_texture) = gpu_images.get(&targets.id_buffer) else {
        return;
    };

    let uniform = EdgeDetectionUniform {
        outline_color: Vec4::new(
            settings.color.red,
            settings.color.green,
            settings.color.blue,
            1.0,
        ),
        thickness: settings.thickness,
        texture_size: Vec2::new(id_texture.texture_descriptor.size.width as f32, id_texture.texture_descriptor.size.height as f32),
        _padding: 0.0,
    };

    // Create uniform buffer using encase for proper alignment
    let mut buffer = bevy::render::render_resource::encase::UniformBuffer::new(Vec::new());
    buffer.write(&uniform).unwrap();
    let uniform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("edge_detection_uniform_buffer"),
        contents: buffer.as_ref(),
        usage: BufferUsages::UNIFORM,
    });

    commands.insert_resource(EdgeDetectionPrepared {
        uniform_buffer,
        id_texture_view: id_texture.texture_view.clone(),
    });
}

fn clear_prepare_edge_detection_resources(mut commands: Commands) {
    commands.remove_resource::<EdgeDetectionPrepared>();
}
