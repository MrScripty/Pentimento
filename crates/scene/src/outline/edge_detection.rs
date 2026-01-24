//! Edge detection post-process for Surface ID outline rendering
//!
//! This module implements a render graph node that reads the ID buffer
//! and composites orange outlines onto the scene where entity IDs differ.
//! Uses the standard Bevy post-processing pattern with ViewTarget::post_process_write().

use bevy::asset::embedded_asset;
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::core_pipeline::FullscreenShader;
use bevy::prelude::*;
use bevy::render::{
    extract_component::ExtractComponentPlugin,
    extract_resource::ExtractResourcePlugin,
    render_asset::RenderAssets,
    render_graph::{
        NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
    },
    render_resource::{
        binding_types::{sampler, texture_2d, uniform_buffer},
        BindGroupEntries, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
        Buffer, BufferInitDescriptor, BufferUsages, CachedRenderPipelineId, ColorTargetState,
        ColorWrites, FragmentState, MultisampleState, Operations, PipelineCache, PrimitiveState,
        RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler,
        SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, TextureFormat,
        TextureSampleType,
    },
    renderer::{RenderContext, RenderDevice},
    texture::GpuImage,
    view::ViewTarget,
    Render, RenderApp, RenderSystems,
};

use super::outline_settings::OutlineSettings;
use super::OutlineCamera;
use super::OutlineRenderTargets;

/// Plugin for edge detection post-processing
pub struct EdgeDetectionPlugin;

impl Plugin for EdgeDetectionPlugin {
    fn build(&self, app: &mut App) {
        // Embed the shader
        embedded_asset!(app, "shaders/edge_detection.wgsl");

        // Extract OutlineCamera component to render world
        app.add_plugins(ExtractComponentPlugin::<OutlineCamera>::default());
        app.add_plugins(ExtractResourcePlugin::<OutlineSettings>::default());
        app.add_plugins(ExtractResourcePlugin::<OutlineRenderTargets>::default());

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            bevy::log::warn!("EdgeDetectionPlugin: No RenderApp available!");
            return;
        };

        bevy::log::info!("EdgeDetectionPlugin: Setting up render graph node");

        // Use ViewNodeRunner with ViewQuery filtering to OutlineCamera
        // This ensures proper per-view execution in Core3d subgraph (required for WASM/WebGL2)
        render_app
            .add_render_graph_node::<ViewNodeRunner<EdgeDetectionNode>>(Core3d, EdgeDetectionLabel);
        render_app.add_render_graph_edges(
            Core3d,
            (
                Node3d::Tonemapping,
                EdgeDetectionLabel,
                Node3d::EndMainPassPostProcessing,
            ),
        );

        render_app.add_systems(Render, prepare_edge_detection.in_set(RenderSystems::Prepare));
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            bevy::log::warn!("EdgeDetectionPlugin::finish: No RenderApp available!");
            return;
        };

        render_app.init_resource::<EdgeDetectionPipeline>();
        bevy::log::info!("EdgeDetectionPlugin: Pipeline initialized");
    }
}

/// Render graph label for edge detection
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct EdgeDetectionLabel;

/// Uniform data for edge detection shader
#[derive(Clone, Copy, ShaderType)]
pub struct EdgeDetectionUniform {
    pub outline_color: Vec4,
    pub thickness: f32,
    pub texture_size: Vec2,
    pub _padding: f32,
}

/// Render graph node for edge detection
/// Uses the standard Bevy post-processing pattern with ViewTarget
#[derive(Default)]
pub struct EdgeDetectionNode;

impl ViewNode for EdgeDetectionNode {
    /// Query ViewTarget to use post_process_write() for proper compositing
    type ViewQuery = &'static ViewTarget;

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        view_target: bevy::ecs::query::QueryItem<'w, 'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        let Some(settings) = world.get_resource::<OutlineSettings>() else {
            return Ok(());
        };

        if !settings.enabled {
            return Ok(());
        }

        let Some(prepared) = world.get_resource::<EdgeDetectionPrepared>() else {
            return Ok(());
        };

        let Some(pipeline) = world.get_resource::<EdgeDetectionPipeline>() else {
            return Ok(());
        };
        let pipeline_cache = world.resource::<PipelineCache>();

        let Some(render_pipeline) = pipeline_cache.get_render_pipeline(pipeline.pipeline_id) else {
            return Ok(());
        };

        // Use ViewTarget's post_process_write() for proper ping-pong buffer handling
        // This returns source (current scene) and destination (where we write)
        let post_process = view_target.post_process_write();

        // Create bind group with:
        // - uniforms
        // - id_buffer (for edge detection)
        // - scene source texture (to composite onto)
        let bind_group = render_context.render_device().create_bind_group(
            "edge_detection_bind_group",
            &pipeline.layout,
            &BindGroupEntries::sequential((
                prepared.uniform_buffer.as_entire_binding(),
                &prepared.id_texture_view,
                &pipeline.sampler,
                post_process.source,  // Scene texture to read from
                &pipeline.sampler,    // Re-use sampler for scene
            )),
        );

        // Render to ViewTarget's destination (composited scene + outlines)
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("edge_detection_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,  // Write to ViewTarget's destination
                resolve_target: None,
                ops: Operations::default(),
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_render_pipeline(render_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

/// Pipeline for edge detection
#[derive(Resource)]
pub struct EdgeDetectionPipeline {
    pub layout: BindGroupLayout,
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
                texture_2d(TextureSampleType::Float { filterable: true }),  // ID buffer
                sampler(SamplerBindingType::Filtering),                     // ID sampler
                texture_2d(TextureSampleType::Float { filterable: true }),  // Scene texture
                sampler(SamplerBindingType::Filtering),                     // Scene sampler
            ),
        );

        // Create the descriptor for the pipeline
        let layout_descriptor = BindGroupLayoutDescriptor::new(
            "edge_detection_bind_group_layout",
            &layout_entries.to_vec(),
        );

        // Create the actual layout for bind group creation
        let layout = render_device.create_bind_group_layout(
            "edge_detection_bind_group_layout",
            &layout_entries,
        );

        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        let shader = world
            .load_asset("embedded://pentimento_scene/outline/shaders/edge_detection.wgsl");

        let fullscreen_shader = world.resource::<FullscreenShader>();
        let vertex_state = fullscreen_shader.to_vertex_state();

        let pipeline_id =
            world
                .resource_mut::<PipelineCache>()
                .queue_render_pipeline(RenderPipelineDescriptor {
                    label: Some("edge_detection_pipeline".into()),
                    layout: vec![layout_descriptor],
                    vertex: vertex_state,
                    fragment: Some(FragmentState {
                        shader,
                        shader_defs: vec![],
                        entry_point: Some("fragment".into()),
                        targets: vec![Some(ColorTargetState {
                            // Use SDR format for WebGL2 compatibility
                            format: TextureFormat::Rgba8UnormSrgb,
                            blend: None,
                            write_mask: ColorWrites::ALL,
                        })],
                    }),
                    primitive: PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: MultisampleState::default(),
                    push_constant_ranges: vec![],
                    zero_initialize_workgroup_memory: false,
                });

        Self {
            layout,
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
        texture_size: Vec2::new(
            id_texture.size.width as f32,
            id_texture.size.height as f32,
        ),
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
