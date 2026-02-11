//! Depth view mode — replaces scene color with linearized greyscale depth.
//!
//! When enabled, a fullscreen post-process pass reads the depth buffer and
//! outputs greyscale (white = near, black = far). Selection outlines and
//! gizmos still render on top. Shadows and AO are disabled while active
//! since the lit scene output is overwritten.

use bevy::asset::embedded_asset;
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::FullscreenShader;
use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::render::{
    extract_component::{ExtractComponent, ExtractComponentPlugin},
    extract_resource::{ExtractResource, ExtractResourcePlugin},
    render_graph::{
        NodeRunError, RenderGraphContext, RenderGraphExt, RenderLabel, ViewNode, ViewNodeRunner,
    },
    render_resource::{
        BindGroupEntries, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
        BindingType, Buffer, BufferBindingType, BufferInitDescriptor, BufferUsages,
        CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState, MultisampleState,
        Operations, PipelineCache, PrimitiveState, RenderPassColorAttachment,
        RenderPassDescriptor, RenderPipelineDescriptor, ShaderStages, ShaderType, TextureFormat,
        TextureSampleType, TextureViewDimension,
    },
    renderer::{RenderContext, RenderDevice},
    view::ViewTarget,
    Render, RenderApp, RenderSystems,
};

use crate::ambient_occlusion::SceneAmbientOcclusion;
use crate::lighting::SunLight;
use crate::camera::MainCamera;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Render graph label for the depth view pass.
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct DepthViewLabel;

/// Marker component added to the main camera so the depth view node can
/// filter to the correct view.  Extracted to the render world automatically.
#[derive(Component, Clone, ExtractComponent)]
pub struct DepthViewCamera;

/// Settings for depth view mode.  Extracted to the render world each frame.
#[derive(Resource, Clone, ExtractResource)]
pub struct DepthViewSettings {
    pub enabled: bool,
    /// Cached previous shadow state so we can restore on toggle-off.
    shadows_were_enabled: bool,
    /// Cached previous AO state.
    ao_was_enabled: bool,
}

impl Default for DepthViewSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            shadows_were_enabled: true,
            ao_was_enabled: false,
        }
    }
}

/// Computed scene depth bounds, updated each frame when depth view is active.
/// Kept separate from `DepthViewSettings` to avoid triggering `is_changed()`
/// on the settings resource every frame.
#[derive(Resource, Clone, ExtractResource)]
pub struct DepthViewBounds {
    /// Camera near clipping plane (for depth linearization).
    pub near_plane: f32,
    /// Nearest scene depth in view-space units (for gradient normalization).
    pub scene_near: f32,
    /// Farthest scene depth in view-space units (for gradient normalization).
    pub scene_far: f32,
}

impl Default for DepthViewBounds {
    fn default() -> Self {
        Self {
            near_plane: 0.1,
            scene_near: 0.1,
            scene_far: 100.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct DepthViewPlugin;

impl Plugin for DepthViewPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/depth_view.wgsl");

        app.init_resource::<DepthViewSettings>();
        app.init_resource::<DepthViewBounds>();
        app.add_plugins(ExtractComponentPlugin::<DepthViewCamera>::default());
        app.add_plugins(ExtractResourcePlugin::<DepthViewSettings>::default());
        app.add_plugins(ExtractResourcePlugin::<DepthViewBounds>::default());

        // Main-world systems that toggle DepthPrepass, disable costly effects,
        // and compute scene depth bounds for gradient normalization.
        app.add_systems(Update, (
            compute_scene_depth_bounds,
            sync_depth_prepass,
            toggle_expensive_features,
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("DepthViewPlugin: No RenderApp available");
            return;
        };

        render_app
            .add_render_graph_node::<ViewNodeRunner<DepthViewNode>>(Core3d, DepthViewLabel);

        // Insert between Tonemapping and EndMainPassPostProcessing.
        // EdgeDetectionPlugin (if present) will add its own edge
        // DepthViewLabel → EdgeDetectionLabel so outlines render on top.
        render_app.add_render_graph_edges(
            Core3d,
            (
                Node3d::Tonemapping,
                DepthViewLabel,
                Node3d::EndMainPassPostProcessing,
            ),
        );

        render_app.add_systems(Render, prepare_depth_view.in_set(RenderSystems::Prepare));

        info!("DepthViewPlugin: render graph node registered");
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.init_resource::<DepthViewPipeline>();
        info!("DepthViewPlugin: pipeline initialized");
    }
}

// ---------------------------------------------------------------------------
// Main-world systems
// ---------------------------------------------------------------------------

/// Add or remove `DepthPrepass` on the main camera so Bevy generates a
/// sampleable depth texture only when we need it.
fn sync_depth_prepass(
    mut commands: Commands,
    settings: Res<DepthViewSettings>,
    camera_query: Query<Entity, With<MainCamera>>,
    prepass_query: Query<Entity, (With<MainCamera>, With<DepthPrepass>)>,
) {
    if !settings.is_changed() {
        return;
    }
    for entity in camera_query.iter() {
        if settings.enabled {
            if !prepass_query.contains(entity) {
                commands.entity(entity).insert(DepthPrepass);
            }
        } else if prepass_query.contains(entity) {
            commands.entity(entity).remove::<DepthPrepass>();
        }
    }
}

/// Disable shadows and AO while depth view is active (their output is
/// overwritten anyway). Restores previous state when toggled off.
fn toggle_expensive_features(
    mut settings: ResMut<DepthViewSettings>,
    mut sun_query: Query<&mut DirectionalLight, With<SunLight>>,
    mut ao_resource: ResMut<SceneAmbientOcclusion>,
) {
    if !settings.is_changed() {
        return;
    }

    if settings.enabled {
        // Capture current state before disabling.
        for light in sun_query.iter() {
            settings.shadows_were_enabled = light.shadows_enabled;
        }
        settings.ao_was_enabled = ao_resource.settings.enabled;

        for mut light in sun_query.iter_mut() {
            light.shadows_enabled = false;
        }
        if ao_resource.settings.enabled {
            ao_resource.settings.enabled = false;
            ao_resource.dirty = true;
        }
    } else {
        // Restore previous state.
        for mut light in sun_query.iter_mut() {
            light.shadows_enabled = settings.shadows_were_enabled;
        }
        if settings.ao_was_enabled {
            ao_resource.settings.enabled = true;
            ao_resource.dirty = true;
        }
    }
}

/// Compute the scene depth bounds from mesh AABBs projected into camera
/// view-space.  Updates `DepthViewBounds` each frame so the depth gradient
/// automatically adapts to the actual scene content.
fn compute_scene_depth_bounds(
    settings: Res<DepthViewSettings>,
    mut bounds: ResMut<DepthViewBounds>,
    mesh_query: Query<(&Aabb, &GlobalTransform), With<Mesh3d>>,
    camera_query: Query<(&GlobalTransform, &Projection), With<MainCamera>>,
) {
    if !settings.enabled {
        return;
    }

    let Ok((camera_transform, projection)) = camera_query.single() else {
        return;
    };

    // Read the actual camera near plane from the projection.
    let near_plane = match projection {
        Projection::Perspective(p) => p.near,
        Projection::Orthographic(o) => o.near,
        _ => 0.1,
    };
    bounds.near_plane = near_plane;

    // World-to-view transform.
    let world_to_view = camera_transform.affine().inverse();

    let mut min_depth = f32::MAX;
    let mut max_depth = f32::MIN;
    let mut found_any = false;

    for (aabb, global_transform) in mesh_query.iter() {
        let center = aabb.center;
        let he = aabb.half_extents;

        // 8 corners of the AABB in local space.
        let corners = [
            center + Vec3A::new(-he.x, -he.y, -he.z),
            center + Vec3A::new(-he.x, -he.y,  he.z),
            center + Vec3A::new(-he.x,  he.y, -he.z),
            center + Vec3A::new(-he.x,  he.y,  he.z),
            center + Vec3A::new( he.x, -he.y, -he.z),
            center + Vec3A::new( he.x, -he.y,  he.z),
            center + Vec3A::new( he.x,  he.y, -he.z),
            center + Vec3A::new( he.x,  he.y,  he.z),
        ];

        // Transform local → world → view and track depth.
        let model = global_transform.affine();
        for corner in &corners {
            let world_pos = model.transform_point3a(*corner);
            let view_pos = world_to_view.transform_point3a(world_pos);
            // Bevy looks along -Z in view space; depth = -z.
            let depth = -view_pos.z;
            if depth > 0.0 {
                min_depth = min_depth.min(depth);
                max_depth = max_depth.max(depth);
                found_any = true;
            }
        }
    }

    if found_any && max_depth > min_depth {
        // 5% padding so boundary objects aren't pure white/black.
        let range = max_depth - min_depth;
        let padding = range * 0.05;
        bounds.scene_near = (min_depth - padding).max(near_plane);
        bounds.scene_far = max_depth + padding;
    } else if found_any {
        // Degenerate: all geometry at the same depth.
        bounds.scene_near = (min_depth * 0.9).max(near_plane);
        bounds.scene_far = max_depth * 1.1;
    } else {
        // No visible geometry — sensible fallback.
        bounds.scene_near = near_plane;
        bounds.scene_far = 100.0;
    }
}

// ---------------------------------------------------------------------------
// Render-world node
// ---------------------------------------------------------------------------

/// Uniform data passed to the depth view shader.
#[derive(Clone, Copy, ShaderType)]
pub struct DepthViewUniform {
    /// Camera near clipping plane (for linearization: linear_z = near_plane / raw_depth).
    pub near_plane: f32,
    /// Nearest scene depth in view-space units (for gradient normalization).
    pub scene_near: f32,
    /// Farthest scene depth in view-space units (for gradient normalization).
    pub scene_far: f32,
    pub _padding0: f32,
}

#[derive(Default)]
pub struct DepthViewNode;

impl ViewNode for DepthViewNode {
    type ViewQuery = (
        &'static ViewTarget,
        Option<&'static DepthViewCamera>,
    );

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (view_target, depth_view_camera): bevy::ecs::query::QueryItem<'w, 'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        // Only run on the main camera.
        if depth_view_camera.is_none() {
            return Ok(());
        }

        let Some(settings) = world.get_resource::<DepthViewSettings>() else {
            return Ok(());
        };
        if !settings.enabled {
            return Ok(());
        }

        let Some(prepared) = world.get_resource::<DepthViewPrepared>() else {
            return Ok(());
        };
        let Some(pipeline_res) = world.get_resource::<DepthViewPipeline>() else {
            return Ok(());
        };
        let pipeline_cache = world.resource::<PipelineCache>();
        let Some(render_pipeline) =
            pipeline_cache.get_render_pipeline(pipeline_res.pipeline_id)
        else {
            return Ok(());
        };

        let post_process = view_target.post_process_write();

        let bind_group = render_context.render_device().create_bind_group(
            "depth_view_bind_group",
            &pipeline_res.layout,
            &BindGroupEntries::sequential((
                prepared.uniform_buffer.as_entire_binding(),
                &prepared.depth_texture_view,
            )),
        );

        let mut render_pass =
            render_context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("depth_view_pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: post_process.destination,
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

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

#[derive(Resource)]
pub struct DepthViewPipeline {
    pub layout: BindGroupLayout,
    pub pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for DepthViewPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        // @binding(0) — uniform buffer
        // @binding(1) — depth texture (texture_depth_multisampled_2d when MSAA active)
        let entries_vec = vec![
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(DepthViewUniform::min_size()),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Depth,
                    view_dimension: TextureViewDimension::D2,
                    multisampled: true,
                },
                count: None,
            },
        ];

        let layout_descriptor =
            BindGroupLayoutDescriptor::new("depth_view_bind_group_layout", &entries_vec);

        let layout = render_device.create_bind_group_layout(
            "depth_view_bind_group_layout",
            &entries_vec,
        );

        let shader = world
            .load_asset("embedded://pentimento_scene/depth_view/shaders/depth_view.wgsl");

        let fullscreen_shader = world.resource::<FullscreenShader>();
        let vertex_state = fullscreen_shader.to_vertex_state();

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("depth_view_pipeline".into()),
                layout: vec![layout_descriptor],
                vertex: vertex_state,
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    entry_point: Some("fragment".into()),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::Rgba16Float,
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
            pipeline_id,
        }
    }
}

// ---------------------------------------------------------------------------
// Prepare system (render world)
// ---------------------------------------------------------------------------

/// Prepared per-frame data consumed by `DepthViewNode`.
#[derive(Resource)]
pub struct DepthViewPrepared {
    pub uniform_buffer: Buffer,
    pub depth_texture_view: bevy::render::render_resource::TextureView,
}

/// Runs in the Render schedule's Prepare set.  Creates the uniform buffer and
/// resolves the depth texture view from the prepass textures.
fn prepare_depth_view(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    settings: Option<Res<DepthViewSettings>>,
    bounds: Option<Res<DepthViewBounds>>,
    views: Query<
        &bevy::core_pipeline::prepass::ViewPrepassTextures,
        With<DepthViewCamera>,
    >,
) {
    let Some(settings) = settings else {
        return;
    };
    if !settings.enabled {
        return;
    }
    let Some(bounds) = bounds else {
        return;
    };

    // Grab the depth texture view from the first matching camera.
    let Some(prepass_textures) = views.iter().next() else {
        return;
    };

    let Some(depth) = prepass_textures.depth.as_ref() else {
        return;
    };

    let depth_texture_view = depth.texture.default_view.clone();

    let uniform = DepthViewUniform {
        near_plane: bounds.near_plane,
        scene_near: bounds.scene_near,
        scene_far: bounds.scene_far,
        _padding0: 0.0,
    };

    let mut buffer =
        bevy::render::render_resource::encase::UniformBuffer::new(Vec::new());
    buffer.write(&uniform).unwrap();

    let uniform_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("depth_view_uniform_buffer"),
        contents: buffer.as_ref(),
        usage: BufferUsages::UNIFORM,
    });

    commands.insert_resource(DepthViewPrepared {
        uniform_buffer,
        depth_texture_view,
    });
}
