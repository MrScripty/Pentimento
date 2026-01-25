Claude’s Plan
Plan: Fix Selection Outline in WASM/Electron Mode
Problem
The selected object outline works in native (CEF, overlay) mode, but not in WASM (Electron).

Root Cause (Corrected After Research)
The current implementation renders to a separate outline_buffer texture and tries to display it via a Sprite. This approach doesn't work reliably in WASM/WebGL2 because:

Sprite rendering of render-target textures has known reliability issues in Bevy
The official Bevy post-processing pattern uses ViewTarget::post_process_write() to composite directly onto the main render target - not a separate texture with Sprite overlay
The correct approach: Use ViewTarget::post_process_write() to:

Get the current scene as source texture
Sample both source (scene) and id_buffer (selection IDs)
Render composited result (scene + outlines) to destination
Implementation Plan
Change 1: Update EdgeDetectionNode to use ViewTarget pattern
File: crates/scene/src/outline/edge_detection.rs

Change the ViewNode to use the standard Bevy post-processing pattern:


impl ViewNode for EdgeDetectionNode {
    // Query ViewTarget to get access to post_process_write()
    type ViewQuery = &'static ViewTarget;

    fn run<'w>(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        view_target: QueryItem<'w, Self::ViewQuery>,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        // ... existing resource checks ...

        // Get source/destination from ViewTarget
        let post_process = view_target.post_process_write();

        // Create bind group with:
        // - uniforms
        // - id_buffer (for edge detection)
        // - scene source texture (post_process.source)
        // - samplers
        let bind_group = render_context.render_device().create_bind_group(
            "edge_detection_bind_group",
            &pipeline.layout,
            &BindGroupEntries::sequential((
                prepared.uniform_buffer.as_entire_binding(),
                &prepared.id_texture_view,
                &pipeline.sampler,
                post_process.source,  // Scene texture to composite onto
                &pipeline.sampler,
            )),
        );

        // Render to destination (composited scene + outlines)
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("edge_detection_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,  // Write to ViewTarget's destination
                resolve_target: None,
                ops: Operations::default(),
                depth_slice: None,
            })],
            // ...
        });
        // ...
    }
}
Change 2: Update shader to composite scene + outlines
File: crates/scene/src/outline/shaders/edge_detection.wgsl

Update shader to sample both ID buffer and scene, then composite:


@group(0) @binding(0) var<uniform> uniforms: EdgeDetectionUniform;
@group(0) @binding(1) var id_texture: texture_2d<f32>;
@group(0) @binding(2) var id_sampler: sampler;
@group(0) @binding(3) var scene_texture: texture_2d<f32>;  // ADD
@group(0) @binding(4) var scene_sampler: sampler;          // ADD

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample scene
    let scene_color = textureSample(scene_texture, scene_sampler, in.uv);

    // Edge detection on ID buffer
    let is_edge = detect_edge(in.uv);

    // Composite: scene + outline where edges detected
    if is_edge {
        return uniforms.outline_color;
    } else {
        return scene_color;
    }
}
Change 3: Update pipeline bind group layout
File: crates/scene/src/outline/edge_detection.rs

Add scene texture binding to the layout:


let layout_entries = BindGroupLayoutEntries::sequential(
    ShaderStages::FRAGMENT,
    (
        uniform_buffer::<EdgeDetectionUniform>(false),
        texture_2d(TextureSampleType::Float { filterable: true }),  // ID buffer
        sampler(SamplerBindingType::Filtering),
        texture_2d(TextureSampleType::Float { filterable: true }),  // Scene texture (ADD)
        sampler(SamplerBindingType::Filtering),                     // Scene sampler (ADD)
    ),
);
Change 4: Update pipeline target format
The target format must match ViewTarget's format (HDR or SDR depending on camera config):


// Use view_target.main_texture_format() or a dynamic approach
format: TextureFormat::Rgba8UnormSrgb,  // May need to be dynamic
Change 5: Remove Sprite overlay system
File: crates/scene/src/outline/mod.rs

Remove the setup_outline_overlay system and OutlineOverlay marker since we no longer need the Sprite approach. Also remove outline_buffer from OutlineRenderTargets - we only need id_buffer.

Change 6: Remove Camera2d for outline overlay
File: crates/scene/src/lib.rs

The Camera2d that was added specifically for rendering the outline Sprite can be removed (unless needed for other UI purposes).

Summary of Changes
File	Change	Reason
edge_detection.rs	Use ViewTarget::post_process_write() pattern	Proper Bevy post-processing approach
edge_detection.wgsl	Sample scene + ID buffer, composite output	Render outlines on top of scene
mod.rs	Remove Sprite overlay, simplify to just id_buffer	No longer needed with ViewTarget approach
lib.rs	Remove outline-specific Camera2d	No longer needed
Verification
Build: ./launcher.sh --electron --build
Run: ./launcher.sh --electron
Test:
Click on Cube, Sphere, or Torus
Should see orange outline around selected object
Scene should render normally (no grey screen, no red debug overlay)
Sources
Bevy Post-Processing Example
ViewTarget documentation
PR #6415: ViewTarget post_process_write
User approved the plan
Update Todos

Update EdgeDetectionNode to use ViewTarget::post_process_write() pattern

Update shader to composite scene + outlines

Update pipeline bind group layout for scene texture

Remove Sprite overlay system from mod.rs

Remove Camera2d for outline overlay from lib.rs

Build and test in Electron mode
Let me start by reading the current edge_detection.rs file and updating it to use the ViewTarget pattern.

Read edge_detection.rs
Now let me rewrite the edge_detection.rs file to use the ViewTarget pattern. I need to:

Change ViewQuery to use ViewTarget
Update the bind group to include the scene texture
Update the render pass to write to ViewTarget's destination
Update the pipeline layout
Write edge_detection.rs
311 lines
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
        view_target: bevy::ecs::query::QueryItem<'w, Self::ViewQuery>,
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
                            // Use standard HDR format that ViewTarget uses
                            // This should match what the camera is using
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
Update Todos

Update EdgeDetectionNode to use ViewTarget::post_process_write() pattern

Update shader to composite scene + outlines

Update pipeline bind group layout for scene texture

Remove Sprite overlay system from mod.rs

Remove Camera2d for outline overlay from lib.rs

Build and test in Electron mode
Now let me update the shader to sample both the ID buffer and scene texture, then composite.

Read edge_detection.wgsl
Write edge_detection.wgsl
82 lines
// Edge detection shader for Surface ID outline rendering
// Composites outlines onto the scene using ViewTarget post-processing pattern
// Samples both ID buffer (for edge detection) and scene texture (for passthrough)

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EdgeDetectionUniform {
    outline_color: vec4<f32>,
    thickness: f32,
    texture_size: vec2<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: EdgeDetectionUniform;

@group(0) @binding(1)
var id_texture: texture_2d<f32>;

@group(0) @binding(2)
var id_sampler: sampler;

@group(0) @binding(3)
var scene_texture: texture_2d<f32>;

@group(0) @binding(4)
var scene_sampler: sampler;

// Sample the ID at a pixel offset
fn sample_id(uv: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;
    return textureSample(id_texture, id_sampler, uv + offset * pixel_size);
}

// Check if the current pixel is an edge (ID differs from any neighbor)
fn is_edge(uv: vec2<f32>) -> bool {
    let center_id = sample_id(uv, vec2<f32>(0.0, 0.0));

    // If center pixel has no ID (black/transparent), it's not an edge
    if center_id.a < 0.01 {
        return false;
    }

    let thickness = uniforms.thickness;

    // Check neighbors in a cross pattern at the configured thickness
    let offsets = array<vec2<f32>, 4>(
        vec2<f32>(-thickness, 0.0),   // left
        vec2<f32>(thickness, 0.0),    // right
        vec2<f32>(0.0, -thickness),   // up
        vec2<f32>(0.0, thickness),    // down
    );

    for (var i = 0; i < 4; i++) {
        let neighbor_id = sample_id(uv, offsets[i]);

        // Edge if neighbor has different ID (including transparent/background)
        // Compare RGB values to detect ID boundaries
        let id_diff = abs(center_id.rgb - neighbor_id.rgb);
        if id_diff.r > 0.01 || id_diff.g > 0.01 || id_diff.b > 0.01 {
            return true;
        }
    }

    return false;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the scene (passthrough by default)
    let scene_color = textureSample(scene_texture, scene_sampler, in.uv);

    // Check if this pixel is an edge in the ID buffer
    if is_edge(in.uv) {
        // Return outline color at edges
        return uniforms.outline_color;
    }

    // Otherwise pass through the scene unchanged
    return scene_color;
}
Update Todos

Update EdgeDetectionNode to use ViewTarget::post_process_write() pattern

Update shader to composite scene + outlines

Update pipeline bind group layout for scene texture

Remove Sprite overlay system from mod.rs

Remove Camera2d for outline overlay from lib.rs

Build and test in Electron mode
Now let me update mod.rs to remove the Sprite overlay system and simplify the OutlineRenderTargets to only have id_buffer.

Read mod.rs
Now I need to update mod.rs to:

Remove the outline_buffer from OutlineRenderTargets
Remove the setup_outline_overlay function
Remove the OutlineOverlay marker and related sprite code
Update handle_window_resize to not deal with sprite or outline_buffer
Write mod.rs
317 lines
//! Surface ID (Cryptomatte) selection outline rendering
//!
//! Renders pixel-accurate orange outlines around selected 3D objects using
//! a Surface ID / Cryptomatte-style approach:
//! 1. ID Pass: Render selected objects to a texture with entity IDs as colors
//! 2. Edge Detection: Post-process shader finds ID boundaries and composites onto scene
//!
//! This approach is WebGL2-compatible for WASM builds.
//! Uses Bevy's standard post-processing pattern with ViewTarget::post_process_write().

use bevy::asset::embedded_asset;
use bevy::asset::RenderAssetUsages;
use bevy::camera::ClearColorConfig;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

mod edge_detection;
mod id_material;
mod outline_settings;

pub use id_material::{EntityIdMaterial, RenderToIdBuffer};
pub use outline_settings::OutlineSettings;

/// Marker component for cameras that need outline post-processing
/// This is extracted to the render world and used by EdgeDetectionNode's ViewQuery
#[derive(Component, Clone, ExtractComponent)]
pub struct OutlineCamera;

use crate::camera::MainCamera;
use crate::selection::Selected;
use edge_detection::EdgeDetectionPlugin;
use id_material::entity_to_color;

/// Resource holding the render targets for outline rendering
#[derive(Resource, Clone, ExtractResource)]
pub struct OutlineRenderTargets {
    /// Texture where entity IDs are rendered
    pub id_buffer: Handle<Image>,
}

/// Marker for the ID buffer camera
#[derive(Component)]
pub struct IdBufferCamera;

/// Plugin for Surface ID selection outlines
pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        // Embed the entity ID shader
        embedded_asset!(app, "shaders/entity_id.wgsl");

        app.init_resource::<OutlineSettings>()
            .add_plugins(MaterialPlugin::<EntityIdMaterial>::default())
            .add_plugins(EdgeDetectionPlugin)
            .add_systems(Startup, setup_outline_system)
            .add_systems(
                Update,
                (
                    sync_id_camera_transform,
                    sync_id_mirror_transforms,
                    add_selected_to_id_buffer,
                    remove_deselected_from_id_buffer,
                    handle_window_resize,
                    debug_outline_resources,
                )
                    .chain(),
            );
    }
}

/// Debug system to verify resources exist (runs once on first selection)
fn debug_outline_resources(
    targets: Option<Res<OutlineRenderTargets>>,
    settings: Option<Res<OutlineSettings>>,
    mirrors: Query<Entity, With<IdBufferMirror>>,
    mut logged: Local<bool>,
) {
    // Only log once when we have mirrors (i.e., something is selected)
    if mirrors.is_empty() || *logged {
        return;
    }
    *logged = true;

    info!("DEBUG: OutlineRenderTargets exists: {}", targets.is_some());
    info!("DEBUG: OutlineSettings exists: {}", settings.is_some());
    info!("DEBUG: Mirror count: {}", mirrors.iter().count());
    if let Some(t) = targets {
        info!("DEBUG: ID buffer handle: {:?}", t.id_buffer);
    }
}

/// Initialize the outline system with render targets and ID camera
fn setup_outline_system(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    windows: Query<&Window>,
    main_camera: Query<(&Transform, &OrbitCamera), With<MainCamera>>,
) {
    let Ok(window) = windows.single() else {
        warn!("No window found for outline system setup");
        return;
    };

    let width = window.resolution.physical_width().max(1);
    let height = window.resolution.physical_height().max(1);

    // Create ID buffer render target (entity IDs as colors)
    let id_buffer = create_render_texture(width, height, TextureFormat::Rgba8Unorm, &mut images);

    commands.insert_resource(OutlineRenderTargets {
        id_buffer: id_buffer.clone(),
    });

    // Get main camera transform for ID camera
    let main_transform = main_camera.single().map(|(t, _)| *t).unwrap_or_else(|_| {
        Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)
    });

    // Spawn ID buffer camera (renders selected objects to ID texture)
    // Use Reinhard tonemapping for WASM/WebGL2 compatibility (TonyMcMapface requires tonemapping_luts)
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1, // Render before main camera
            clear_color: ClearColorConfig::Custom(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ..default()
        },
        RenderTarget::Image(id_buffer.into()),
        main_transform,
        // Only render entities on layer 1 (selected objects)
        RenderLayers::layer(1),
        IdBufferCamera,
        Tonemapping::Reinhard,
    ));

    info!("Surface ID outline system initialized ({}x{})", width, height);
}

/// Create a render target texture
fn create_render_texture(
    width: u32,
    height: u32,
    format: TextureFormat,
    images: &mut Assets<Image>,
) -> Handle<Image> {
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        format,
        RenderAssetUsages::all(),
    );

    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    images.add(image)
}

/// Sync ID camera transform with main camera
fn sync_id_camera_transform(
    main_camera: Query<&Transform, (With<MainCamera>, Without<IdBufferCamera>)>,
    mut id_camera: Query<&mut Transform, With<IdBufferCamera>>,
) {
    let Ok(main_transform) = main_camera.single() else {
        return;
    };
    let Ok(mut id_transform) = id_camera.single_mut() else {
        return;
    };

    *id_transform = *main_transform;
}

/// When an entity is selected, set up ID buffer rendering
fn add_selected_to_id_buffer(
    mut commands: Commands,
    mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    added_selected: Query<(Entity, &Mesh3d), Added<Selected>>,
    meshes: Res<Assets<Mesh>>,
) {
    for (entity, mesh_handle) in added_selected.iter() {
        let entity_color = entity_to_color(entity);

        // Create ID material for this entity
        let id_material = id_materials.add(EntityIdMaterial {
            entity_id: id_material::EntityIdUniform { entity_color },
        });

        // Clone the mesh for the ID pass rendering
        // We need a separate entity on layer 1 with the ID material
        if let Some(mesh) = meshes.get(&mesh_handle.0) {
            commands.spawn((
                Mesh3d(mesh_handle.0.clone()),
                MeshMaterial3d(id_material),
                // Will be synced with the original entity's transform
                Transform::default(),
                GlobalTransform::default(),
                // Required for mesh to be visible to any camera
                Visibility::default(),
                // Only visible to ID camera
                RenderLayers::layer(1),
                RenderToIdBuffer { entity_color },
                // Track which entity this is for
                IdBufferMirror { source: entity },
                Pickable::IGNORE,
            ));

            info!(
                "Added entity {:?} to ID buffer with color {:?}",
                entity, entity_color
            );
        }
    }
}

/// Component linking an ID buffer mirror to its source entity
#[derive(Component)]
pub struct IdBufferMirror {
    pub source: Entity,
}

/// Update ID buffer mirror transforms to match their source entities
fn sync_id_mirror_transforms(
    source_query: Query<&GlobalTransform, With<Selected>>,
    mut mirror_query: Query<(&IdBufferMirror, &mut Transform)>,
) {
    for (mirror, mut transform) in mirror_query.iter_mut() {
        if let Ok(source_transform) = source_query.get(mirror.source) {
            // Copy the global transform as local (since mirror has no parent)
            let (scale, rotation, translation) = source_transform.to_scale_rotation_translation();
            transform.translation = translation;
            transform.rotation = rotation;
            transform.scale = scale;
        }
    }
}

/// Remove ID buffer entities when their source is deselected
fn remove_deselected_from_id_buffer(
    mut commands: Commands,
    mirror_query: Query<(Entity, &IdBufferMirror)>,
    selected_query: Query<&Selected>,
) {
    for (mirror_entity, mirror) in mirror_query.iter() {
        // If source entity no longer has Selected component, remove the mirror
        if selected_query.get(mirror.source).is_err() {
            commands.entity(mirror_entity).despawn();
            info!(
                "Removed ID buffer mirror for deselected entity {:?}",
                mirror.source
            );
        }
    }
}

/// Handle window resize by recreating render targets
fn handle_window_resize(
    mut commands: Commands,
    windows: Query<&Window, Changed<Window>>,
    mut images: ResMut<Assets<Image>>,
    targets: Option<ResMut<OutlineRenderTargets>>,
    id_camera: Query<Entity, With<IdBufferCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Some(mut targets) = targets else {
        return;
    };

    let width = window.resolution.physical_width().max(1);
    let height = window.resolution.physical_height().max(1);

    // Check if resize is needed
    if let Some(id_image) = images.get(&targets.id_buffer) {
        if id_image.width() == width && id_image.height() == height {
            return;
        }
    }

    // Create new ID buffer
    let new_id_buffer = create_render_texture(width, height, TextureFormat::Rgba8Unorm, &mut images);

    // Update camera target component
    if let Ok(camera_entity) = id_camera.single() {
        commands
            .entity(camera_entity)
            .insert(RenderTarget::Image(new_id_buffer.clone().into()));
    }

    // Remove old texture
    images.remove(&targets.id_buffer);

    targets.id_buffer = new_id_buffer;

    info!("Resized outline render targets to {}x{}", width, height);
}

// Re-export OrbitCamera for setup
use crate::camera::OrbitCamera;
Update Todos

Update EdgeDetectionNode to use ViewTarget::post_process_write() pattern

Update shader to composite scene + outlines

Update pipeline bind group layout for scene texture

Remove Sprite overlay system from mod.rs

Remove Camera2d for outline overlay from lib.rs

Build and test in Electron mode
Now let me update lib.rs to remove the Camera2d that was only for the outline overlay.

Read lib.rs
I see the Camera2d was added for rendering the outline overlay (sprite). Since we're no longer using the Sprite overlay approach, we can remove this Camera2d. However, I should be careful - bevy_ui might spawn its own Camera2d. Let me check if this Camera2d is actually needed for any other purpose. Based on the code, it was specifically for rendering the outline sprite overlay, so we can remove it.

Edit lib.rs
Removed 2 lines
Edit lib.rs
Removed 15 lines
Update Todos

Update EdgeDetectionNode to use ViewTarget::post_process_write() pattern

Update shader to composite scene + outlines

Update pipeline bind group layout for scene texture

Remove Sprite overlay system from mod.rs

Remove Camera2d for outline overlay from lib.rs

Build and test in Electron mode
Now let me build the project to check for any compilation errors.

Bash Build the Electron WASM project
IN
cd /media/jeremy/OrangeCream/Linux\ Software/Pentimento && ./launcher.sh --electron --build 2>&1

OUT
Exit code 101
Building Pentimento in Electron mode...
Building Svelte UI...
Installing npm dependencies...

up to date, audited 91 packages in 353ms

16 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities

> pentimento-ui@0.1.0 build
> vite build

vite v6.4.1 building for production...
transforming...
2:56:27 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:157:4 Unused CSS selector ".nav-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
155:     .nav-button:hover,
156:     .nav-button.active,
157:     .nav-button.pentimento-hover {
                                     ^
158:         background: rgba(255, 255, 255, 0.1);
159:         color: white;
2:56:27 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:195:4 Unused CSS selector ".dropdown-item.pentimento-hover"
https://svelte.dev/e/css_unused_selector
193: 
194:     .dropdown-item:hover,
195:     .dropdown-item.pentimento-hover {
                                        ^
196:         background: rgba(255, 255, 255, 0.1);
197:     }
2:56:27 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:228:4 Unused CSS selector ".tool-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
226: 
227:     .tool-button:hover,
228:     .tool-button.pentimento-hover {
                                      ^
229:         background: rgba(255, 255, 255, 0.15);
230:         color: white;
2:56:27 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:56:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
54: 
55:                 <div class="property">
56:                     <label class="property-label">Metallic</label>
                                                                      ^
57:                     <input
58:                         type="range"
2:56:27 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:70:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
68: 
69:                 <div class="property">
70:                     <label class="property-label">Roughness</label>
                                                                       ^
71:                     <input
72:                         type="range"
✓ 114 modules transformed.
rendering chunks...
computing gzip size...
../dist/ui/index.html         3.46 kB │ gzip:  1.26 kB
../dist/ui/assets/index.css  10.80 kB │ gzip:  2.95 kB
../dist/ui/assets/index.js   37.55 kB │ gzip: 13.74 kB
✓ built in 500ms
Building Bevy WASM...
   Compiling pentimento-scene v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/scene)
error[E0107]: type alias takes 2 lifetime arguments but 1 lifetime argument was supplied
   --> crates/scene/src/outline/edge_detection.rs:110:40
    |
110 |         view_target: bevy::ecs::query::QueryItem<'w, Self::ViewQuery>,
    |                                        ^^^^^^^^^ -- supplied 1 lifetime argument
    |                                        |
    |                                        expected 2 lifetime arguments
    |
note: type alias defined here, with 2 lifetime parameters: `'w`, `'s`
   --> /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_ecs-0.18.0/src/query/fetch.rs:363:10
    |
363 | pub type QueryItem<'w, 's, Q> = <Q as QueryData>::Item<'w, 's>;
    |          ^^^^^^^^^ --  --
help: add missing lifetime argument
    |
110 |         view_target: bevy::ecs::query::QueryItem<'w, 'w, Self::ViewQuery>,
    |                                                    ++++

warning: variable does not need to be mutable
   --> crates/scene/src/outline/mod.rs:103:5
    |
103 |     mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    |     ----^^^^^^^^^^^^
    |     |
    |     help: remove this `mut`
    |
    = note: `#[warn(unused_mut)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `id_materials`
   --> crates/scene/src/outline/mod.rs:103:5
    |
103 |     mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    |     ^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_id_materials`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `mesh`
   --> crates/scene/src/outline/mod.rs:206:21
    |
206 |         if let Some(mesh) = meshes.get(&mesh_handle.0) {
    |                     ^^^^ help: if this is intentional, prefix it with an underscore: `_mesh`

For more information about this error, try `rustc --explain E0107`.
warning: `pentimento-scene` (lib) generated 3 warnings
error: could not compile `pentimento-scene` (lib) due to 1 previous error; 3 warnings emitted
I need to fix the lifetime parameter issue. In Bevy 0.18, QueryItem takes 2 lifetime arguments.

Edit edge_detection.rs
Modified
Bash Build the Electron WASM project
IN
cd /media/jeremy/OrangeCream/Linux\ Software/Pentimento && ./launcher.sh --electron --build 2>&1

OUT
Building Pentimento in Electron mode...
Building Svelte UI...
Installing npm dependencies...

up to date, audited 91 packages in 339ms

16 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities

> pentimento-ui@0.1.0 build
> vite build

vite v6.4.1 building for production...
transforming...
2:56:40 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:157:4 Unused CSS selector ".nav-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
155:     .nav-button:hover,
156:     .nav-button.active,
157:     .nav-button.pentimento-hover {
                                     ^
158:         background: rgba(255, 255, 255, 0.1);
159:         color: white;
2:56:40 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:195:4 Unused CSS selector ".dropdown-item.pentimento-hover"
https://svelte.dev/e/css_unused_selector
193: 
194:     .dropdown-item:hover,
195:     .dropdown-item.pentimento-hover {
                                        ^
196:         background: rgba(255, 255, 255, 0.1);
197:     }
2:56:40 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:228:4 Unused CSS selector ".tool-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
226: 
227:     .tool-button:hover,
228:     .tool-button.pentimento-hover {
                                      ^
229:         background: rgba(255, 255, 255, 0.15);
230:         color: white;
2:56:40 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:56:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
54: 
55:                 <div class="property">
56:                     <label class="property-label">Metallic</label>
                                                                      ^
57:                     <input
58:                         type="range"
2:56:40 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:70:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
68: 
69:                 <div class="property">
70:                     <label class="property-label">Roughness</label>
                                                                       ^
71:                     <input
72:                         type="range"
✓ 114 modules transformed.
rendering chunks...
computing gzip size...
../dist/ui/index.html         3.46 kB │ gzip:  1.26 kB
../dist/ui/assets/index.css  10.80 kB │ gzip:  2.95 kB
../dist/ui/assets/index.js   37.55 kB │ gzip: 13.74 kB
✓ built in 512ms
Building Bevy WASM...
   Compiling pentimento-scene v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/scene)
warning: variable does not need to be mutable
   --> crates/scene/src/outline/mod.rs:103:5
    |
103 |     mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    |     ----^^^^^^^^^^^^
    |     |
    |     help: remove this `mut`
    |
    = note: `#[warn(unused_mut)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `id_materials`
   --> crates/scene/src/outline/mod.rs:103:5
    |
103 |     mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    |     ^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_id_materials`
    |
    = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: unused variable: `mesh`
   --> crates/scene/src/outline/mod.rs:206:21
    |
206 |         if let Some(mesh) = meshes.get(&mesh_handle.0) {
    |                     ^^^^ help: if this is intentional, prefix it with an underscore: `_mesh`

warning: field `entity_color` is never read
  --> crates/scene/src/outline/id_material.rs:59:9
   |
57 | pub struct RenderToIdBuffer {
   |            ---------------- field in this struct
58 |     /// The entity's ID color
59 |     pub entity_color: Vec4,
   |         ^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `pentimento-scene` (lib) generated 4 warnings (run `cargo fix --lib -p pentimento-scene` to apply 3 suggestions)
   Compiling pentimento-wasm v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/app-wasm)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.18s
Running wasm-bindgen...
Setting up Electron...
Build complete (Electron mode)
WASM output: dist/wasm/
UI output: dist/ui/
Electron: src-electron/
Build succeeded. Let me clean up those warnings first, then you can test.

Edit mod.rs
Removed 1 line
Edit mod.rs
Modified
Bash Build the Electron WASM project
IN
cd /media/jeremy/OrangeCream/Linux\ Software/Pentimento && ./launcher.sh --electron --build 2>&1

OUT
Building Pentimento in Electron mode...
Building Svelte UI...
Installing npm dependencies...

up to date, audited 91 packages in 352ms

16 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities

> pentimento-ui@0.1.0 build
> vite build

vite v6.4.1 building for production...
transforming...
2:57:12 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:157:4 Unused CSS selector ".nav-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
155:     .nav-button:hover,
156:     .nav-button.active,
157:     .nav-button.pentimento-hover {
                                     ^
158:         background: rgba(255, 255, 255, 0.1);
159:         color: white;
2:57:12 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:195:4 Unused CSS selector ".dropdown-item.pentimento-hover"
https://svelte.dev/e/css_unused_selector
193: 
194:     .dropdown-item:hover,
195:     .dropdown-item.pentimento-hover {
                                        ^
196:         background: rgba(255, 255, 255, 0.1);
197:     }
2:57:12 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:228:4 Unused CSS selector ".tool-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
226: 
227:     .tool-button:hover,
228:     .tool-button.pentimento-hover {
                                      ^
229:         background: rgba(255, 255, 255, 0.15);
230:         color: white;
2:57:12 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:56:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
54: 
55:                 <div class="property">
56:                     <label class="property-label">Metallic</label>
                                                                      ^
57:                     <input
58:                         type="range"
2:57:12 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:70:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
68: 
69:                 <div class="property">
70:                     <label class="property-label">Roughness</label>
                                                                       ^
71:                     <input
72:                         type="range"
✓ 114 modules transformed.
rendering chunks...
computing gzip size...
../dist/ui/index.html         3.46 kB │ gzip:  1.26 kB
../dist/ui/assets/index.css  10.80 kB │ gzip:  2.95 kB
../dist/ui/assets/index.js   37.55 kB │ gzip: 13.74 kB
✓ built in 519ms
Building Bevy WASM...
   Compiling pentimento-scene v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/scene)
warning: field `entity_color` is never read
  --> crates/scene/src/outline/id_material.rs:59:9
   |
57 | pub struct RenderToIdBuffer {
   |            ---------------- field in this struct
58 |     /// The entity's ID color
59 |     pub entity_color: Vec4,
   |         ^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `pentimento-scene` (lib) generated 1 warning
   Compiling pentimento-wasm v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/app-wasm)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.90s
Running wasm-bindgen...
Setting up Electron...
Build complete (Electron mode)
WASM output: dist/wasm/
UI output: dist/ui/
Electron: src-electron/
Build succeeded with just one minor warning about an unused field. Now you can test it.

Update Todos

Update EdgeDetectionNode to use ViewTarget::post_process_write() pattern

Update shader to composite scene + outlines

Update pipeline bind group layout for scene texture

Remove Sprite overlay system from mod.rs

Remove Camera2d for outline overlay from lib.rs

Build and test in Electron mode
Build succeeded. Run ./launcher.sh --electron to test. Click on the Cube, Sphere, or Torus to select them - you should now see orange outlines around the selected objects.

The key changes made:

EdgeDetectionNode now uses ViewTarget::post_process_write() to properly composite outlines onto the scene (standard Bevy post-processing pattern)
Shader now samples both the ID buffer and the scene texture, rendering outlines where edges are detected
Removed Sprite overlay approach which was unreliable in WASM/WebGL2
Removed Camera2d that was only needed for the Sprite overlay
Pentimento WASM bridge initialized
pentimento_wasm.js:908 INFO crates/scene/src/outline/edge_detection.rs:55 EdgeDetectionPlugin: Setting up render graph node
pentimento_wasm.js:908 INFO /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_winit-0.18.0/src/system.rs:68 Creating new window App (0v0)
pentimento_wasm.js:908 INFO /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_render-0.18.0/src/renderer/mod.rs:283 AdapterInfo { name: "ANGLE (Intel, Mesa Intel(R) Graphics (RPL-P), OpenGL 4.6)", vendor: 32902, device: 0, device_type: IntegratedGpu, driver: "", driver_info: "WebGL 2.0 (OpenGL ES 3.0 Chromium)", backend: Gl }
VM9:1 Pentimento: WASM loaded!
pentimento_wasm.js:908 INFO /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_render-0.18.0/src/batching/gpu_preprocessing.rs:1135 GPU preprocessing is not supported on this device. Falling back to CPU preprocessing.
pentimento_wasm.js:908 WARN /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_core_pipeline-0.18.0/src/oit/resolve/mod.rs:72 OrderIndependentTransparencyPlugin not loaded. GPU lacks support: DownlevelFlags::FRAGMENT_WRITABLE_STORAGE.
pentimento_wasm.js:908 WARN /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_pbr-0.18.0/src/ssao/mod.rs:70 ScreenSpaceAmbientOcclusionPlugin not loaded. GPU lacks support: Limits::max_storage_textures_per_shader_stage is less than 5.
pentimento_wasm.js:908 INFO /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_pbr-0.18.0/src/light_probe/generate.rs:130 Disabling EnvironmentMapGenerationPlugin because compute is not supported on this platform. This is safe to ignore if you are not using EnvironmentMapGenerationPlugin.
pentimento_wasm.js:908 WARN /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_pbr-0.18.0/src/atmosphere/mod.rs:131 AtmospherePlugin not loaded. GPU lacks support for compute shaders.
pentimento_wasm.js:908 INFO crates/scene/src/outline/edge_detection.rs:80 EdgeDetectionPlugin: Pipeline initialized
pentimento_wasm.js:908 INFO crates/scene/src/outline/mod.rs:143 Surface ID outline system initialized (1280x720)
pentimento_wasm.js:908 INFO crates/scene/src/lighting.rs:84 Scene lighting initialized
pentimento_wasm.js:908 INFO crates/scene/src/lib.rs:152 Scene initialized with test objects
pentimento_wasm.js:908 INFO crates/scene/src/lighting.rs:122 Scene lighting updated
pentimento_wasm.js:908 ERROR  Handling wgpu errors as fatal by default log.target = "wgpu::backend::wgpu_core";log.module_path = "wgpu::backend::wgpu_core";log.file = "/home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wgpu-27.0.1/src/backend/wgpu_core.rs";log.line = 2568;
pentimento_wasm.js:594 panicked at /home/jeremy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wgpu-27.0.1/src/backend/wgpu_core.rs:2568:18:
wgpu error: Validation Error

Caused by:
  In a CommandEncoder
    In a set_pipeline command
      Render pipeline targets are incompatible with render pass
        Incompatible color attachments at indices [0]: the RenderPass uses textures with formats [Some(Rgba8UnormSrgb)] but the RenderPipeline with 'edge_detection_pipeline' label uses attachments with formats [Some(Rgba16Float)]



Stack:

Error
    at __wbg_new_8a6f238a6ece86ea (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm.js:1002:25)
    at pentimento_wasm.wasm.console_error_panic_hook::Error::new::__wbg_new_8a6f238a6ece86ea::hbc64df06860b64ea externref shim (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[430065]:0x4599479)
    at pentimento_wasm.wasm.console_error_panic_hook::Error::new::h1b10279b912f4f0a (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[256448]:0x3e8d6b9)
    at pentimento_wasm.wasm.console_error_panic_hook::hook_impl::hc270b8ab3f72d510 (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[81284]:0x2be7b70)
    at pentimento_wasm.wasm.console_error_panic_hook::hook::h4cf66d380e7f63e3 (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[375314]:0x4416b95)
    at pentimento_wasm.wasm.core::ops::function::Fn::call::h412f49d6eda63f69 (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[322314]:0x4207393)
    at pentimento_wasm.wasm.std::panicking::panic_with_hook::hf90632d5c7102557 (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[76290]:0x2af2d16)
    at pentimento_wasm.wasm.std::panicking::panic_handler::{{closure}}::he5eb13023b639916 (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[111257]:0x30fd42a)
    at pentimento_wasm.wasm.std::sys::backtrace::__rust_end_short_backtrace::hdaee13ec4dd4f348 (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[437212]:0x45adee8)
    at pentimento_wasm.wasm.__rustc[d9b87f19e823c0ef]::rust_begin_unwind (file:///media/jeremy/OrangeCream/Linux%20Software/Pentimento/dist/ui/wasm/pentimento_wasm_bg.wasm:wasm-function[329284]:0x4253c5d)


__wbg_error_7534b8e9a36f1ab4 @ pentimento_wasm.js:594
pentimento_wasm_bg.wasm:0x45b98bd Uncaught RuntimeError: unreachable
    at pentimento_wasm.wasm.__rustc[d9b87f19e823c0ef]::__rust_abort (pentimento_wasm_bg.wasm:0x45b98bd)
    at pentimento_wasm.wasm.__rustc[d9b87f19e823c0ef]::__rust_start_panic (pentimento_wasm_bg.wasm:0x45b522a)
    at pentimento_wasm.wasm.__rustc[d9b87f19e823c0ef]::rust_panic (pentimento_wasm_bg.wasm:0x459f311)
    at pentimento_wasm.wasm.std::panicking::panic_with_hook::hf90632d5c7102557 (pentimento_wasm_bg.wasm:0x2af2d4f)
    at pentimento_wasm.wasm.std::panicking::panic_handler::{{closure}}::he5eb13023b639916 (pentimento_wasm_bg.wasm:0x30fd42a)
    at pentimento_wasm.wasm.std::sys::backtrace::__rust_end_short_backtrace::hdaee13ec4dd4f348 (pentimento_wasm_bg.wasm:0x45adee8)
    at pentimento_wasm.wasm.__rustc[d9b87f19e823c0ef]::rust_begin_unwind (pentimento_wasm_bg.wasm:0x4253c5d)
    at pentimento_wasm.wasm.core::panicking::panic_fmt::hcb6b2b4be1f4be38 (pentimento_wasm_bg.wasm:0x4253ce7)
    at pentimento_wasm.wasm.wgpu::backend::wgpu_core::default_error_handler::h8152fc3cf99111bc (pentimento_wasm_bg.wasm:0x280e2b0)
    at pentimento_wasm.wasm.wgpu::backend::wgpu_core::ErrorSinkRaw::handle_error_or_return_handler::h21efe889c68014ee (pentimento_wasm_bg.wasm:0x1303cf7)
The error is clear: there's a texture format mismatch. The render pass uses Rgba8UnormSrgb (WebGL2's SDR format) but the pipeline expects Rgba16Float (HDR format). WebGL2 doesn't support HDR render targets.

Let me fix the pipeline to use the correct format for WebGL2.

Edit edge_detection.rs
Modified
Bash Build with fixed texture format
IN
cd /media/jeremy/OrangeCream/Linux\ Software/Pentimento && ./launcher.sh --electron --build 2>&1

OUT
Building Pentimento in Electron mode...
Building Svelte UI...
Installing npm dependencies...

up to date, audited 91 packages in 337ms

16 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities

> pentimento-ui@0.1.0 build
> vite build

vite v6.4.1 building for production...
transforming...
2:59:39 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:157:4 Unused CSS selector ".nav-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
155:     .nav-button:hover,
156:     .nav-button.active,
157:     .nav-button.pentimento-hover {
                                     ^
158:         background: rgba(255, 255, 255, 0.1);
159:         color: white;
2:59:39 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:195:4 Unused CSS selector ".dropdown-item.pentimento-hover"
https://svelte.dev/e/css_unused_selector
193: 
194:     .dropdown-item:hover,
195:     .dropdown-item.pentimento-hover {
                                        ^
196:         background: rgba(255, 255, 255, 0.1);
197:     }
2:59:39 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:228:4 Unused CSS selector ".tool-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
226: 
227:     .tool-button:hover,
228:     .tool-button.pentimento-hover {
                                      ^
229:         background: rgba(255, 255, 255, 0.15);
230:         color: white;
2:59:39 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:56:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
54: 
55:                 <div class="property">
56:                     <label class="property-label">Metallic</label>
                                                                      ^
57:                     <input
58:                         type="range"
2:59:39 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:70:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
68: 
69:                 <div class="property">
70:                     <label class="property-label">Roughness</label>
                                                                       ^
71:                     <input
72:                         type="range"
✓ 114 modules transformed.
rendering chunks...
computing gzip size...
../dist/ui/index.html         3.46 kB │ gzip:  1.26 kB
../dist/ui/assets/index.css  10.80 kB │ gzip:  2.95 kB
../dist/ui/assets/index.js   37.55 kB │ gzip: 13.74 kB
✓ built in 571ms
Building Bevy WASM...
   Compiling pentimento-scene v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/scene)
warning: field `entity_color` is never read
  --> crates/scene/src/outline/id_material.rs:59:9
   |
57 | pub struct RenderToIdBuffer {
   |            ---------------- field in this struct
58 |     /// The entity's ID color
59 |     pub entity_color: Vec4,
   |         ^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `pentimento-scene` (lib) generated 1 warning
   Compiling pentimento-wasm v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/app-wasm)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.95s
Running wasm-bindgen...
Setting up Electron...
Build complete (Electron mode)
WASM output: dist/wasm/
UI output: dist/ui/
Electron: src-electron/
Build succeeded. Run ./launcher.sh --electron to test again. The texture format is now correctly set to Rgba8UnormSrgb which WebGL2 supports.

Outline works! There is a bug, but i have seen this bug before when we were developing the outline for CEF mode. Then an object is selected the outline errodes inwards every frame causing it to fill the pixel space that the object occupies. 

Although the CEF update to fix this problem which is in the latest commit may not directly apply, you might hve an idea how to fix the same/simular issue in WASM thanks to it. Here is the commit message:

The post-process ping-pong system caused outline dilation because the
scene texture accumulated outline pixels between frames. Fixed by:

- Create separate outline_buffer texture for edge detection output
- Render edge detection to outline_buffer instead of ViewTarget
- Composite outline via UI ImageNode overlay (ZIndex layering)
- Clear outline_buffer each frame (no feedback possible)

The shader now only reads from id_buffer (frame-local) and outputs
transparent/outline to its own texture. UI compositing handles the
final overlay without touching the main render pipeline.

There is a known bug where the outline will appear for a single frame at center of screen when selecting another object. This is not a serious problem and will be fixed later.
I see the issue. The problem is the same as what was fixed in CEF mode - the post_process_write() pattern causes feedback because the scene texture accumulates outline pixels between frames.

The fix in CEF mode was to render to a separate outline_buffer texture and composite via UI. But we already tried that approach with Sprite and it didn't work in WASM.

However, there's a simpler solution: we can clear the ID buffer each frame and only draw outlines where there's actually a selected object's ID. The issue is that the edge detection is finding "edges" between the accumulated outline pixels and the background.

Let me look at a different approach - instead of using the ViewTarget ping-pong, we should ensure the outline only appears where the ID buffer has actual content, not where previous frame outlines were. The shader should:

Only draw outlines where the ID buffer has a non-zero ID
Pass through the scene color everywhere else
But wait - the real issue is that post_process_write() flips textures, so the "source" contains the previous frame's output (which includes outlines). The outline pixels then get detected as edges again.

The solution is to not use post_process_write() for the outline, but instead render the outline to a separate texture and composite it. Since Sprite didn't work, let me try a different approach - we can use alpha blending directly in the post-process pass, but read from a separate snapshot of the scene before any outlines are applied.

Actually, the simplest fix is to ensure the shader only draws outlines at pixels where the ID buffer indicates a selected object boundary - not where the scene already has outline colors. Let me modify the edge detection logic to be more precise.

Read edge_detection.wgsl
The current logic draws an outline if the center pixel has an ID and any neighbor has a different ID. The issue is that the scene texture (from post_process_write().source) contains the previous frame's render, which includes the outlines we drew.

The key insight from the CEF fix is: the ID buffer is cleared each frame (it renders fresh), so it doesn't have the feedback problem. The issue is that when we read from the scene texture, we're reading outlines from the previous frame.

The fix is to only draw outlines at the boundary pixels - pixels where the center has an ID but the neighbor does NOT have an ID (or vice versa). This ensures we only draw on the actual silhouette edge, not filling inward.

Let me modify the shader to draw outlines outside the object (where neighbor has ID but center doesn't) rather than inside:

Write edge_detection.wgsl
91 lines
// Edge detection shader for Surface ID outline rendering
// Composites outlines onto the scene using ViewTarget post-processing pattern
// Samples both ID buffer (for edge detection) and scene texture (for passthrough)
//
// IMPORTANT: Outlines are drawn OUTSIDE the object (where center has no ID but
// a neighbor does) to avoid feedback loop erosion from ping-pong buffers.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EdgeDetectionUniform {
    outline_color: vec4<f32>,
    thickness: f32,
    texture_size: vec2<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: EdgeDetectionUniform;

@group(0) @binding(1)
var id_texture: texture_2d<f32>;

@group(0) @binding(2)
var id_sampler: sampler;

@group(0) @binding(3)
var scene_texture: texture_2d<f32>;

@group(0) @binding(4)
var scene_sampler: sampler;

// Sample the ID at a pixel offset
fn sample_id(uv: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;
    return textureSample(id_texture, id_sampler, uv + offset * pixel_size);
}

// Check if this pixel should have an outline drawn
// Returns true if this pixel is OUTSIDE an object but adjacent to one
// This draws outlines outside objects, avoiding feedback loop erosion
fn should_draw_outline(uv: vec2<f32>) -> bool {
    let center_id = sample_id(uv, vec2<f32>(0.0, 0.0));

    // If center pixel HAS an ID, don't draw outline here (we draw outside)
    // This prevents the erosion feedback loop
    if center_id.a > 0.01 {
        return false;
    }

    let thickness = uniforms.thickness;

    // Check neighbors - if any neighbor has an ID, draw outline here
    let offsets = array<vec2<f32>, 8>(
        vec2<f32>(-thickness, 0.0),    // left
        vec2<f32>(thickness, 0.0),     // right
        vec2<f32>(0.0, -thickness),    // up
        vec2<f32>(0.0, thickness),     // down
        vec2<f32>(-thickness, -thickness),  // top-left
        vec2<f32>(thickness, -thickness),   // top-right
        vec2<f32>(-thickness, thickness),   // bottom-left
        vec2<f32>(thickness, thickness),    // bottom-right
    );

    for (var i = 0; i < 8; i++) {
        let neighbor_id = sample_id(uv, offsets[i]);

        // If any neighbor has an ID, this background pixel is on the outline
        if neighbor_id.a > 0.01 {
            return true;
        }
    }

    return false;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the scene (passthrough by default)
    let scene_color = textureSample(scene_texture, scene_sampler, in.uv);

    // Check if this pixel should have an outline
    // Outlines are drawn OUTSIDE objects (where center has no ID but neighbor does)
    if should_draw_outline(in.uv) {
        // Return outline color
        return uniforms.outline_color;
    }

    // Otherwise pass through the scene unchanged
    return scene_color;
}
Bash Build with fixed outline shader
IN
cd /media/jeremy/OrangeCream/Linux\ Software/Pentimento && ./launcher.sh --electron --build 2>&1

OUT
Building Pentimento in Electron mode...
Building Svelte UI...
Installing npm dependencies...

up to date, audited 91 packages in 359ms

16 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities

> pentimento-ui@0.1.0 build
> vite build

vite v6.4.1 building for production...
transforming...
3:04:20 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:56:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
54: 
55:                 <div class="property">
56:                     <label class="property-label">Metallic</label>
                                                                      ^
57:                     <input
58:                         type="range"
3:04:20 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:70:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
68: 
69:                 <div class="property">
70:                     <label class="property-label">Roughness</label>
                                                                       ^
71:                     <input
72:                         type="range"
3:04:20 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:157:4 Unused CSS selector ".nav-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
155:     .nav-button:hover,
156:     .nav-button.active,
157:     .nav-button.pentimento-hover {
                                     ^
158:         background: rgba(255, 255, 255, 0.1);
159:         color: white;
3:04:20 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:195:4 Unused CSS selector ".dropdown-item.pentimento-hover"
https://svelte.dev/e/css_unused_selector
193: 
194:     .dropdown-item:hover,
195:     .dropdown-item.pentimento-hover {
                                        ^
196:         background: rgba(255, 255, 255, 0.1);
197:     }
3:04:20 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:228:4 Unused CSS selector ".tool-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
226: 
227:     .tool-button:hover,
228:     .tool-button.pentimento-hover {
                                      ^
229:         background: rgba(255, 255, 255, 0.15);
230:         color: white;
✓ 114 modules transformed.
rendering chunks...
computing gzip size...
../dist/ui/index.html         3.46 kB │ gzip:  1.26 kB
../dist/ui/assets/index.css  10.80 kB │ gzip:  2.95 kB
../dist/ui/assets/index.js   37.55 kB │ gzip: 13.74 kB
✓ built in 526ms
Building Bevy WASM...
   Compiling pentimento-scene v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/scene)
warning: field `entity_color` is never read
  --> crates/scene/src/outline/id_material.rs:59:9
   |
57 | pub struct RenderToIdBuffer {
   |            ---------------- field in this struct
58 |     /// The entity's ID color
59 |     pub entity_color: Vec4,
   |         ^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `pentimento-scene` (lib) generated 1 warning
   Compiling pentimento-wasm v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/app-wasm)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.93s
Running wasm-bindgen...
Setting up Electron...
Build complete (Electron mode)
WASM output: dist/wasm/
UI output: dist/ui/
Electron: src-electron/
Build succeeded. Run ./launcher.sh --electron to test.

The key change is that outlines are now drawn outside the object silhouette instead of inside:

Before: Draw outline where center HAS an ID and neighbor differs → causes erosion as outlines accumulate
After: Draw outline where center has NO ID but a neighbor DOES → only affects pixels outside the object, no feedback possible
This should fix the erosion problem because the outline pixels are outside the ID buffer's coverage area, so they can't feed back into the edge detection.

That fix causes the inverse issue where the outline diolates outwards to fill the screen. When working on CEF mode we encountered that same issue as well.
You're right - drawing outside causes outward dilation for the same reason (the outline pixels accumulate in the scene texture each frame).

The fundamental problem is the post_process_write() ping-pong pattern - the scene texture contains previous frame's outlines, which get detected and drawn again.

The CEF fix used a separate texture that gets cleared each frame, then composited via UI overlay. We tried Sprite overlay in WASM but it didn't work. However, there might be another approach.

Let me think about this differently: The ID buffer is fresh each frame (rendered by the ID camera). The problem is reading from the scene texture which has accumulated outlines.

What if we don't sample the scene texture at all for edge detection logic? We only use the ID buffer to decide where to draw, and the scene texture only for passthrough. The key insight is: we should only draw outlines where the ID buffer indicates an edge, and the ID buffer is frame-local (no accumulation).

The issue is that even with correct edge detection, the outline pixels we write become part of the scene for the next frame.

Let me try a different approach: detect edges based purely on the ID buffer, but only draw the outline once by checking if the scene pixel already looks like the outline color. This is a hack but might work:

Write edge_detection.wgsl
89 lines
// Edge detection shader for Surface ID outline rendering
// Composites outlines onto the scene using ViewTarget post-processing pattern
// Samples both ID buffer (for edge detection) and scene texture (for passthrough)
//
// Edge detection is based purely on the ID buffer (which is fresh each frame).
// We draw outlines at the boundary between ID pixels and non-ID pixels.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EdgeDetectionUniform {
    outline_color: vec4<f32>,
    thickness: f32,
    texture_size: vec2<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: EdgeDetectionUniform;

@group(0) @binding(1)
var id_texture: texture_2d<f32>;

@group(0) @binding(2)
var id_sampler: sampler;

@group(0) @binding(3)
var scene_texture: texture_2d<f32>;

@group(0) @binding(4)
var scene_sampler: sampler;

// Sample the ID at a pixel offset
fn sample_id(uv: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;
    return textureSample(id_texture, id_sampler, uv + offset * pixel_size);
}

// Check if the current pixel is on the edge boundary in the ID buffer
// Returns true if center pixel has ID and any neighbor does NOT have ID
fn is_boundary_edge(uv: vec2<f32>) -> bool {
    let center_id = sample_id(uv, vec2<f32>(0.0, 0.0));

    // If center pixel has no ID, it's not part of a selected object
    if center_id.a < 0.01 {
        return false;
    }

    let thickness = uniforms.thickness;

    // Check neighbors in 8 directions
    let offsets = array<vec2<f32>, 8>(
        vec2<f32>(-thickness, 0.0),         // left
        vec2<f32>(thickness, 0.0),          // right
        vec2<f32>(0.0, -thickness),         // up
        vec2<f32>(0.0, thickness),          // down
        vec2<f32>(-thickness, -thickness),  // top-left
        vec2<f32>(thickness, -thickness),   // top-right
        vec2<f32>(-thickness, thickness),   // bottom-left
        vec2<f32>(thickness, thickness),    // bottom-right
    );

    for (var i = 0; i < 8; i++) {
        let neighbor_id = sample_id(uv, offsets[i]);

        // If any neighbor has NO ID (or different ID), this is a boundary pixel
        if neighbor_id.a < 0.01 {
            return true;
        }
    }

    return false;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the scene (passthrough by default)
    let scene_color = textureSample(scene_texture, scene_sampler, in.uv);

    // Check if this pixel is on the edge of a selected object in the ID buffer
    if is_boundary_edge(in.uv) {
        // Blend outline with scene using alpha for anti-aliased look
        // This draws on pixels that HAVE an ID but are at the boundary
        return uniforms.outline_color;
    }

    // Pass through the scene unchanged
    return scene_color;
}
Wait - this is actually the same as the original approach that caused inward erosion. Let me think more carefully about why both directions cause problems:

Drawing inside (where center has ID, neighbor doesn't): The outline overwrites object pixels → next frame those pixels in the scene texture are orange → they still have IDs in the fresh ID buffer → new boundary is detected further inside → erosion inward

Drawing outside (where center has no ID, neighbor does): The outline draws on background pixels → next frame those pixels in scene texture are orange → they DON'T have IDs (ID buffer is fresh) → but now new boundary pixels (further out) see orange neighbors... wait, this shouldn't cause dilation because we only draw where neighbor HAS an ID.

Actually, the dilation issue with "draw outside" is different. Let me reconsider...

The real issue might be that the scene texture from post_process_write() is the cumulative rendered scene, which includes the outline from the previous frame. When we draw outlines outside the object, those orange pixels become part of the scene. Next frame, when we look for boundaries, the ID buffer is fresh (only shows the object), but the scene already has orange pixels around it from last frame.

The fundamental problem is that post_process_write() gives us a scene that already has outlines baked in. We need to either:

Have access to the scene before outlines were added
Use a completely separate overlay system (which Sprite didn't work for)
Let me try a third approach: don't use post_process_write() at all. Instead, get the ViewTarget's main texture directly and draw to a separate destination, then copy back. But actually, that's what the separate texture approach was...

Actually, let me reconsider the Sprite approach. The issue was that the sprite wasn't visible. Maybe the problem was the texture format or the timing. Let me check if we can use a bevy_ui ImageNode instead of Sprite, which might work better in WASM:

Web SearchBevy 0.15 0.18 UiImage ImageNode render target texture WASM WebGL2
OUT
The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). The user provided the following reason for the rejection:  we are using bevy 0.18
Let me try a different approach. Since both inside and outside drawing cause feedback issues, the only real solution is to prevent the feedback.

One idea: compare the scene pixel color to the outline color - if they're already similar, don't draw again. This breaks the feedback loop:

Write edge_detection.wgsl
99 lines
// Edge detection shader for Surface ID outline rendering
// Composites outlines onto the scene using ViewTarget post-processing pattern
//
// To prevent feedback loop (erosion/dilation), we:
// 1. Only draw outlines on pixels that are part of the selected object (have ID)
// 2. Only draw if the scene pixel doesn't already look like the outline color
// This ensures outlines are drawn exactly once and don't accumulate.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EdgeDetectionUniform {
    outline_color: vec4<f32>,
    thickness: f32,
    texture_size: vec2<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: EdgeDetectionUniform;

@group(0) @binding(1)
var id_texture: texture_2d<f32>;

@group(0) @binding(2)
var id_sampler: sampler;

@group(0) @binding(3)
var scene_texture: texture_2d<f32>;

@group(0) @binding(4)
var scene_sampler: sampler;

// Sample the ID at a pixel offset
fn sample_id(uv: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;
    return textureSample(id_texture, id_sampler, uv + offset * pixel_size);
}

// Check if a color is similar to the outline color (already has outline)
fn is_outline_color(color: vec4<f32>) -> bool {
    let diff = abs(color.rgb - uniforms.outline_color.rgb);
    let threshold = 0.1;
    return diff.r < threshold && diff.g < threshold && diff.b < threshold;
}

// Check if the current pixel is on the edge boundary in the ID buffer
// Returns true if center pixel has ID and any neighbor does NOT have ID
fn is_boundary_edge(uv: vec2<f32>) -> bool {
    let center_id = sample_id(uv, vec2<f32>(0.0, 0.0));

    // If center pixel has no ID, it's not part of a selected object
    if center_id.a < 0.01 {
        return false;
    }

    let thickness = uniforms.thickness;

    // Check neighbors in 8 directions
    let offsets = array<vec2<f32>, 8>(
        vec2<f32>(-thickness, 0.0),         // left
        vec2<f32>(thickness, 0.0),          // right
        vec2<f32>(0.0, -thickness),         // up
        vec2<f32>(0.0, thickness),          // down
        vec2<f32>(-thickness, -thickness),  // top-left
        vec2<f32>(thickness, -thickness),   // top-right
        vec2<f32>(-thickness, thickness),   // bottom-left
        vec2<f32>(thickness, thickness),    // bottom-right
    );

    for (var i = 0; i < 8; i++) {
        let neighbor_id = sample_id(uv, offsets[i]);

        // If any neighbor has NO ID, this is a boundary pixel
        if neighbor_id.a < 0.01 {
            return true;
        }
    }

    return false;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the scene
    let scene_color = textureSample(scene_texture, scene_sampler, in.uv);

    // Check if this pixel is on the edge of a selected object
    if is_boundary_edge(in.uv) {
        // Only draw outline if scene pixel doesn't already have the outline color
        // This prevents feedback loop accumulation
        if !is_outline_color(scene_color) {
            return uniforms.outline_color;
        }
    }

    // Pass through the scene unchanged
    return scene_color;
}
Bash Build with feedback prevention shader
IN
cd /media/jeremy/OrangeCream/Linux\ Software/Pentimento && ./launcher.sh --electron --build 2>&1

OUT
Building Pentimento in Electron mode...
Building Svelte UI...
Installing npm dependencies...

up to date, audited 91 packages in 393ms

16 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities

> pentimento-ui@0.1.0 build
> vite build

vite v6.4.1 building for production...
transforming...
3:06:55 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:157:4 Unused CSS selector ".nav-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
155:     .nav-button:hover,
156:     .nav-button.active,
157:     .nav-button.pentimento-hover {
                                     ^
158:         background: rgba(255, 255, 255, 0.1);
159:         color: white;
3:06:55 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:195:4 Unused CSS selector ".dropdown-item.pentimento-hover"
https://svelte.dev/e/css_unused_selector
193: 
194:     .dropdown-item:hover,
195:     .dropdown-item.pentimento-hover {
                                        ^
196:         background: rgba(255, 255, 255, 0.1);
197:     }
3:06:55 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:228:4 Unused CSS selector ".tool-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
226: 
227:     .tool-button:hover,
228:     .tool-button.pentimento-hover {
                                      ^
229:         background: rgba(255, 255, 255, 0.15);
230:         color: white;
3:06:55 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:56:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
54: 
55:                 <div class="property">
56:                     <label class="property-label">Metallic</label>
                                                                      ^
57:                     <input
58:                         type="range"
3:06:55 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:70:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
68: 
69:                 <div class="property">
70:                     <label class="property-label">Roughness</label>
                                                                       ^
71:                     <input
72:                         type="range"
✓ 114 modules transformed.
rendering chunks...
computing gzip size...
../dist/ui/index.html         3.46 kB │ gzip:  1.26 kB
../dist/ui/assets/index.css  10.80 kB │ gzip:  2.95 kB
../dist/ui/assets/index.js   37.55 kB │ gzip: 13.74 kB
✓ built in 595ms
Building Bevy WASM...
   Compiling pentimento-scene v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/scene)
warning: field `entity_color` is never read
  --> crates/scene/src/outline/id_material.rs:59:9
   |
57 | pub struct RenderToIdBuffer {
   |            ---------------- field in this struct
58 |     /// The entity's ID color
59 |     pub entity_color: Vec4,
   |         ^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `pentimento-scene` (lib) generated 1 warning
   Compiling pentimento-wasm v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/app-wasm)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.10s
Running wasm-bindgen...
Setting up Electron...
Build complete (Electron mode)
WASM output: dist/wasm/
UI output: dist/ui/
Electron: src-electron/
Build succeeded. Run ./launcher.sh --electron to test.

This approach adds a check: only draw the outline if the scene pixel doesn't already look like the outline color. This should prevent the feedback loop because:

Frame 1: Boundary detected, scene pixel is object color → draw outline
Frame 2: Same boundary detected, but scene pixel is now orange → don't draw again (already has outline)
The outline should stay stable at the boundary without eroding inward or dilating outward.

that fixed the dialation / erosion problem. The outline now renders correctly around the object. There is one more issue to fix. The outline from the previous frame isnt being cleared. The result is when the camera moves the scene gets painted with orange lines as new outlines are added every frame.
The outline color check prevents erosion/dilation but doesn't clear old outlines when the camera moves. The old orange pixels remain in the scene texture because we're passing them through unchanged.

The issue is that we only draw outlines where the ID buffer indicates a boundary, but we never remove outlines where the ID buffer no longer indicates a boundary (because the object moved in screen space).

The fix is: if a pixel has the outline color but is NOT on a boundary in the current frame's ID buffer, we should restore the scene color underneath. But we don't have access to the "original" scene without outlines.

Let me think about this differently. We need to:

Draw outlines where there's a boundary
Clear outlines where there used to be a boundary but isn't anymore
The problem is we can't distinguish "original scene pixel that happens to be orange" from "outline we drew".

One solution: always restore the scene behind outline pixels by sampling neighboring non-outline pixels. But that's complex.

A simpler solution: if the pixel looks like outline color but is NOT at a boundary, replace it with a sampled neighbor that isn't outline-colored. This effectively "erases" stale outlines:

Write edge_detection.wgsl
150 lines
// Edge detection shader for Surface ID outline rendering
// Composites outlines onto the scene using ViewTarget post-processing pattern
//
// To prevent feedback loop issues:
// 1. Only draw outlines on boundary pixels (where ID exists and neighbor doesn't)
// 2. Only draw if scene pixel doesn't already have outline color (prevents accumulation)
// 3. Clear stale outlines: if pixel has outline color but ISN'T a boundary, restore scene

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct EdgeDetectionUniform {
    outline_color: vec4<f32>,
    thickness: f32,
    texture_size: vec2<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: EdgeDetectionUniform;

@group(0) @binding(1)
var id_texture: texture_2d<f32>;

@group(0) @binding(2)
var id_sampler: sampler;

@group(0) @binding(3)
var scene_texture: texture_2d<f32>;

@group(0) @binding(4)
var scene_sampler: sampler;

// Sample the ID at a pixel offset
fn sample_id(uv: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;
    return textureSample(id_texture, id_sampler, uv + offset * pixel_size);
}

// Sample the scene at a pixel offset
fn sample_scene(uv: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;
    return textureSample(scene_texture, scene_sampler, uv + offset * pixel_size);
}

// Check if a color is similar to the outline color
fn is_outline_color(color: vec4<f32>) -> bool {
    let diff = abs(color.rgb - uniforms.outline_color.rgb);
    let threshold = 0.15;
    return diff.r < threshold && diff.g < threshold && diff.b < threshold;
}

// Check if the current pixel is on the edge boundary in the ID buffer
// Returns true if center pixel has ID and any neighbor does NOT have ID
fn is_boundary_edge(uv: vec2<f32>) -> bool {
    let center_id = sample_id(uv, vec2<f32>(0.0, 0.0));

    // If center pixel has no ID, it's not part of a selected object
    if center_id.a < 0.01 {
        return false;
    }

    let thickness = uniforms.thickness;

    // Check neighbors in 8 directions
    let offsets = array<vec2<f32>, 8>(
        vec2<f32>(-thickness, 0.0),         // left
        vec2<f32>(thickness, 0.0),          // right
        vec2<f32>(0.0, -thickness),         // up
        vec2<f32>(0.0, thickness),          // down
        vec2<f32>(-thickness, -thickness),  // top-left
        vec2<f32>(thickness, -thickness),   // top-right
        vec2<f32>(-thickness, thickness),   // bottom-left
        vec2<f32>(thickness, thickness),    // bottom-right
    );

    for (var i = 0; i < 8; i++) {
        let neighbor_id = sample_id(uv, offsets[i]);

        // If any neighbor has NO ID, this is a boundary pixel
        if neighbor_id.a < 0.01 {
            return true;
        }
    }

    return false;
}

// Find a nearby non-outline pixel to use as replacement for stale outlines
fn find_replacement_color(uv: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / uniforms.texture_size;

    // Sample in a small radius to find a non-outline pixel
    let offsets = array<vec2<f32>, 8>(
        vec2<f32>(-1.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, -1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );

    // Try to find a neighbor that isn't outline-colored
    for (var i = 0; i < 8; i++) {
        let neighbor = textureSample(scene_texture, scene_sampler, uv + offsets[i] * pixel_size);
        if !is_outline_color(neighbor) {
            return neighbor;
        }
    }

    // If all neighbors are outline-colored, try further out
    for (var i = 0; i < 8; i++) {
        let neighbor = textureSample(scene_texture, scene_sampler, uv + offsets[i] * pixel_size * 2.0);
        if !is_outline_color(neighbor) {
            return neighbor;
        }
    }

    // Fallback: return a neutral color (this shouldn't happen often)
    return vec4<f32>(0.2, 0.2, 0.2, 1.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Sample the scene
    let scene_color = textureSample(scene_texture, scene_sampler, in.uv);
    let is_boundary = is_boundary_edge(in.uv);
    let looks_like_outline = is_outline_color(scene_color);

    // Case 1: This IS a boundary pixel
    if is_boundary {
        // Draw outline if not already outline-colored
        if !looks_like_outline {
            return uniforms.outline_color;
        }
        // Already has outline, keep it
        return scene_color;
    }

    // Case 2: This is NOT a boundary pixel
    // If it looks like an outline, it's a stale outline - clear it
    if looks_like_outline {
        return find_replacement_color(in.uv);
    }

    // Normal scene pixel, pass through
    return scene_color;
}
Bash Build with stale outline clearing
IN
cd /media/jeremy/OrangeCream/Linux\ Software/Pentimento && ./launcher.sh --electron --build 2>&1

OUT
Building Pentimento in Electron mode...
Building Svelte UI...
Installing npm dependencies...

up to date, audited 91 packages in 401ms

16 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities

> pentimento-ui@0.1.0 build
> vite build

vite v6.4.1 building for production...
transforming...
3:10:05 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:157:4 Unused CSS selector ".nav-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
155:     .nav-button:hover,
156:     .nav-button.active,
157:     .nav-button.pentimento-hover {
                                     ^
158:         background: rgba(255, 255, 255, 0.1);
159:         color: white;
3:10:05 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:195:4 Unused CSS selector ".dropdown-item.pentimento-hover"
https://svelte.dev/e/css_unused_selector
193: 
194:     .dropdown-item:hover,
195:     .dropdown-item.pentimento-hover {
                                        ^
196:         background: rgba(255, 255, 255, 0.1);
197:     }
3:10:05 p.m. [vite-plugin-svelte] ui/src/lib/components/Toolbar.svelte:228:4 Unused CSS selector ".tool-button.pentimento-hover"
https://svelte.dev/e/css_unused_selector
226: 
227:     .tool-button:hover,
228:     .tool-button.pentimento-hover {
                                      ^
229:         background: rgba(255, 255, 255, 0.15);
230:         color: white;
3:10:05 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:56:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
54: 
55:                 <div class="property">
56:                     <label class="property-label">Metallic</label>
                                                                      ^
57:                     <input
58:                         type="range"
3:10:05 p.m. [vite-plugin-svelte] ui/src/lib/components/SidePanel.svelte:70:20 A form label must be associated with a control
https://svelte.dev/e/a11y_label_has_associated_control
68: 
69:                 <div class="property">
70:                     <label class="property-label">Roughness</label>
                                                                       ^
71:                     <input
72:                         type="range"
✓ 114 modules transformed.
rendering chunks...
computing gzip size...
../dist/ui/index.html         3.46 kB │ gzip:  1.26 kB
../dist/ui/assets/index.css  10.80 kB │ gzip:  2.95 kB
../dist/ui/assets/index.js   37.55 kB │ gzip: 13.74 kB
✓ built in 513ms
Building Bevy WASM...
   Compiling pentimento-scene v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/scene)
warning: field `entity_color` is never read
  --> crates/scene/src/outline/id_material.rs:59:9
   |
57 | pub struct RenderToIdBuffer {
   |            ---------------- field in this struct
58 |     /// The entity's ID color
59 |     pub entity_color: Vec4,
   |         ^^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `pentimento-scene` (lib) generated 1 warning
   Compiling pentimento-wasm v0.1.0 (/media/jeremy/OrangeCream/Linux Software/Pentimento/crates/app-wasm)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.10s
Running wasm-bindgen...
Setting up Electron...
Build complete (Electron mode)
WASM output: dist/wasm/
UI output: dist/ui/
Electron: src-electron/
Build succeeded. Run ./launcher.sh --electron to test.

The shader now has three cases:

Boundary pixel: Draw outline (if not already outline-colored)
Non-boundary pixel that looks like outline: It's a stale outline - replace it with a nearby non-outline color
Normal pixel: Pass through unchanged
This should clear the orange "paint trails" when the camera moves, by finding and replacing stale outline pixels with neighboring scene colors.
