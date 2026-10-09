//! First canonical editable document format. Every load is prepared off-world.
use crate::project_assets::{MaterialDocument, MeshDocument};
use crate::{
    CanvasPlane, CanvasPlaneIdGenerator, OutboundUiMessages, PaintingResource, ProjectionTarget,
    ProjectionTargets,
};
use bevy::{ecs::message::Messages, prelude::*};
use painting::{
    BrushPreset,
    layer::{LayerStack, LayerStackDocument},
};
use pentimento_ipc::{BevyToUi, BlendMode, ProjectCommand, SculptBrushSettings};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PIXELS: usize = 4_194_304;
const MAX_RECORDS: usize = 1_000_000;
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct ProjectObjectId(pub u64);
#[cfg(feature = "sculpting")]
#[derive(Component, Clone)]
pub(crate) struct ProjectSculptGeometry {
    pub mesh: sculpting::ChunkedMesh,
    pub rendered: MeshDocument,
}
#[derive(Resource, Default)]
pub(crate) struct ProjectState {
    path: Option<PathBuf>,
    original: Option<Vec<u8>>,
    next_object_id: u64,
    notice: Option<String>,
    blocked: bool,
    pub(crate) generation: u64,
}
pub fn project_generation(world: &World) -> u64 {
    world
        .get_resource::<ProjectState>()
        .map_or(0, |s| s.generation)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectDocument {
    format: String,
    version: u32,
    next_object_id: u64,
    next_plane_id: u32,
    next_mesh_id: u32,
    next_stroke_id: u64,
    object_counter: u32,
    objects: Vec<ObjectDocument>,
    brush: BrushDocument,
    active_canvas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uv_receiver: Option<u64>,
    live_projection: bool,
    view: Option<ViewDocument>,
    lighting: Option<pentimento_ipc::LightingSettings>,
    ambient_occlusion: Option<pentimento_ipc::AmbientOcclusionSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mesh_brush: Option<MeshBrushDocument>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MeshBrushDocument {
    preset: BrushPreset,
    color: [f32; 4],
    blend: BlendMode,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewDocument {
    target: [f32; 3],
    distance: f32,
    yaw: f32,
    pitch: f32,
    orbit_sensitivity: f32,
    pan_sensitivity: f32,
    zoom_sensitivity: f32,
    min_distance: f32,
    max_distance: f32,
}
impl ViewDocument {
    fn capture(c: &crate::OrbitCamera) -> Self {
        Self {
            target: c.target.to_array(),
            distance: c.distance,
            yaw: c.yaw,
            pitch: c.pitch,
            orbit_sensitivity: c.orbit_sensitivity,
            pan_sensitivity: c.pan_sensitivity,
            zoom_sensitivity: c.zoom_sensitivity,
            min_distance: c.min_distance,
            max_distance: c.max_distance,
        }
    }
    fn restore(&self) -> crate::OrbitCamera {
        crate::OrbitCamera {
            target: Vec3::from_array(self.target),
            distance: self.distance,
            yaw: self.yaw,
            pitch: self.pitch,
            orbit_sensitivity: self.orbit_sensitivity,
            pan_sensitivity: self.pan_sensitivity,
            zoom_sensitivity: self.zoom_sensitivity,
            min_distance: self.min_distance,
            max_distance: self.max_distance,
        }
    }
    fn valid(&self) -> bool {
        let position = self.restore().calculate_position();
        let direction = position - Vec3::from_array(self.target);
        position.is_finite()
            && direction.length_squared().is_finite()
            && direction.length_squared() > 0.0
            && self
                .target
                .iter()
                .chain(&[
                    self.distance,
                    self.yaw,
                    self.pitch,
                    self.orbit_sensitivity,
                    self.pan_sensitivity,
                    self.zoom_sensitivity,
                    self.min_distance,
                    self.max_distance,
                ])
                .all(|v| v.is_finite())
            && self.min_distance > 0.
            && self.max_distance >= self.min_distance
            && self.distance >= self.min_distance
            && self.distance <= self.max_distance
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrushDocument {
    paint: BrushPreset,
    color: [f32; 4],
    blend: BlendMode,
    sculpt: Option<(SculptBrushSettings, Option<f32>)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectDocument {
    id: u64,
    name: Option<String>,
    selectable: Option<String>,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
    hidden: bool,
    mesh: MeshDocument,
    material: MaterialDocument,
    canvas: Option<CanvasDocument>,
    projection: Option<ProjectionDocument>,
    paintable: Option<(u32, StorageDocument)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mesh_uv: Option<crate::project_uv::DirectUvDocument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uv_layers: Option<painting::uv_layers::UvLayersDocument>,
    #[cfg(feature = "sculpting")]
    sculpt: Option<sculpting::chunking::ChunkedDocument>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum StorageDocument {
    Uv(u32, u32),
    Ptex(u32),
}
impl StorageDocument {
    fn capture(v: painting::MeshStorageMode) -> Self {
        match v {
            painting::MeshStorageMode::UvAtlas { resolution: (w, h) } => Self::Uv(w, h),
            painting::MeshStorageMode::Ptex { face_resolution } => Self::Ptex(face_resolution),
        }
    }
    fn restore(&self) -> painting::MeshStorageMode {
        match *self {
            Self::Uv(w, h) => painting::MeshStorageMode::UvAtlas { resolution: (w, h) },
            Self::Ptex(face_resolution) => painting::MeshStorageMode::Ptex { face_resolution },
        }
    }
    fn validate(&self) -> Result<(), String> {
        if match *self {
            Self::Uv(w, h) => w == 0 || h == 0 || w > 1048 || h > 1048,
            Self::Ptex(r) => r == 0 || r > 64,
        } {
            Err("Invalid project mesh paint storage".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanvasDocument {
    plane_id: u32,
    world_width: f32,
    world_height: f32,
    camera_position: Option<[f32; 3]>,
    camera_target: Option<[f32; 3]>,
    layers: LayerStackDocument,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionDocument {
    resolution: (u32, u32),
    layers: Vec<(u32, Vec<[f32; 4]>)>,
}
struct PreparedObject {
    document: ObjectDocument,
    mesh: Mesh,
    #[cfg(feature = "sculpting")]
    sculpt: Option<sculpting::ChunkedMesh>,
}
struct PreparedProject {
    document: ProjectDocument,
    objects: Vec<PreparedObject>,
    painting: PaintingResource,
}

fn capture_mesh_brush(world: &mut World) -> Option<MeshBrushDocument> {
    #[cfg(feature = "mesh_painting")]
    if world
        .query::<&crate::PaintableMesh>()
        .iter(world)
        .next()
        .is_none()
    {
        return None;
    }
    #[cfg(feature = "mesh_painting")]
    return world
        .get_resource::<crate::MeshPaintingResource>()
        .map(|r| MeshBrushDocument {
            preset: r.brush_preset.clone(),
            color: r.brush_color,
            blend: match r.blend_mode {
                painting::BlendMode::Normal => BlendMode::Normal,
                painting::BlendMode::Erase => BlendMode::Erase,
            },
        });
    #[cfg(not(feature = "mesh_painting"))]
    {
        let _ = world;
        None
    }
}
fn idle(world: &World) -> Result<(), String> {
    #[cfg(feature = "mesh_painting")]
    if world
        .get_resource::<crate::MeshPaintingResource>()
        .is_some_and(|r| r.has_active_stroke())
    {
        return Err(
            "Finish or cancel the current DirectUV stroke before project operations".into(),
        );
    }
    if crate::brush_presets::active(world)
        || world
            .get_resource::<crate::GizmoState>()
            .is_some_and(|s| s.is_active)
    {
        return Err(
            "Finish or cancel the current stroke/transform before saving or opening a project."
                .into(),
        );
    }
    #[cfg(feature = "mesh_editing")]
    if world
        .get_resource::<crate::MeshEditState>()
        .is_some_and(|s| s.target_entity.is_some())
    {
        return Err("Exit mesh edit mode before saving or opening a project.".into());
    }
    #[cfg(feature = "mesh_painting")]
    if world
        .get_resource::<crate::MeshPaintState>()
        .is_some_and(|s| s.current_stroke.is_some())
    {
        return Err("Finish the current mesh paint stroke first.".into());
    }
    Ok(())
}
fn document_entities(world: &mut World) -> Vec<Entity> {
    #[cfg(feature = "sculpting")]
    let chunks = world
        .get_resource::<crate::sculpt_mode::SculptingData>()
        .map(|s| s.chunk_entities.clone())
        .unwrap_or_default();
    world
        .query_filtered::<Entity, With<Mesh3d>>()
        .iter(world)
        .filter(|e| {
            #[cfg(feature = "sculpting")]
            if chunks.contains(e) {
                return false;
            }
            #[cfg(feature = "selection")]
            if world.get::<crate::outline::IdBufferMirror>(*e).is_some() {
                return false;
            }
            true
        })
        .collect()
}
fn capture(world: &mut World) -> Result<(ProjectDocument, Vec<(Entity, u64)>), String> {
    #[cfg(feature = "mesh_painting")]
    if world
        .get_resource::<crate::projection_painting::LiveUvPreview>()
        .is_some_and(|p| p.active())
    {
        return Err("Apply or cancel the UV projection preview before saving".into());
    }
    idle(world)?;
    let entities = document_entities(world);
    let mesh_brush = capture_mesh_brush(world);
    let cameras: Vec<_> = world
        .query_filtered::<&crate::OrbitCamera, With<crate::MainCamera>>()
        .iter(world)
        .map(ViewDocument::capture)
        .collect();
    if cameras.len() > 1 {
        return Err("Project v1 supports one main orbit camera".into());
    }
    if entities.len() > 256 {
        return Err("Project object limit exceeded (256)".into());
    }
    let painting = world
        .get_resource::<PaintingResource>()
        .ok_or("Painting state unavailable")?;
    let mut next = world
        .get_resource::<ProjectState>()
        .map_or(1, |s| s.next_object_id.max(1));
    for e in &entities {
        if let Some(id) = world.get::<ProjectObjectId>(*e) {
            next = next.max(id.0.checked_add(1).ok_or("Project object IDs exhausted")?);
        }
    }
    let mut assigned = Vec::new();
    let mut objects = Vec::new();
    let meshes = world
        .get_resource::<Assets<Mesh>>()
        .ok_or("Mesh assets unavailable")?;
    let materials = world
        .get_resource::<Assets<StandardMaterial>>()
        .ok_or("Material assets unavailable")?;
    let images = world
        .get_resource::<Assets<Image>>()
        .ok_or("Image assets unavailable")?;
    for entity in entities {
        if world.get::<ChildOf>(entity).is_some() {
            return Err(
                "Project v1 requires root scene objects; parented objects are unsupported".into(),
            );
        }
        let id = if let Some(id) = world.get::<ProjectObjectId>(entity) {
            id.0
        } else {
            let id = next;
            next = next.checked_add(1).ok_or("Project object IDs exhausted")?;
            assigned.push((entity, id));
            id
        };
        let transform = world.get::<Transform>(entity).copied().unwrap_or_default();
        let mesh_handle = world.get::<Mesh3d>(entity).unwrap();
        let mesh = MeshDocument::capture(
            meshes
                .get(&mesh_handle.0)
                .ok_or("Missing project mesh asset")?,
        )?;
        #[cfg(feature = "sculpting")]
        let sculpt = {
            let active = world
                .get_resource::<crate::SculptState>()
                .filter(|s| s.active && s.target_entity == Some(entity));
            if active.is_some() {
                let chunks = world
                    .get_resource::<crate::sculpt_mode::SculptingData>()
                    .and_then(|s| s.chunked_mesh.as_ref())
                    .ok_or("Missing active sculpt geometry")?;
                Some(chunks.document())
            } else if let Some(saved) = world.get::<ProjectSculptGeometry>(entity) {
                if serde_json::to_vec(&mesh).map_err(|e| e.to_string())?
                    != serde_json::to_vec(&saved.rendered).map_err(|e| e.to_string())?
                {
                    return Err("Sculpt geometry changed outside its owner; re-enter/exit sculpt mode before saving.".into());
                }
                Some(saved.mesh.document())
            } else {
                None
            }
        };
        let canvas = world
            .get::<CanvasPlane>(entity)
            .map(|c| {
                if c.width == 0
                    || c.height == 0
                    || c.width > CanvasPlane::MAX_RESOLUTION
                    || c.height > CanvasPlane::MAX_RESOLUTION
                {
                    return Err("Invalid canvas dimensions before project capture".to_string());
                }
                let layers = painting
                    .get_pipeline(c.plane_id)
                    .map(|p| p.layers.document())
                    .unwrap_or_else(|| LayerStack::new(c.width, c.height).document());
                if layers.width != c.width || layers.height != c.height {
                    return Err("Canvas pipeline dimensions disagree with the scene".into());
                }
                Ok(CanvasDocument {
                    plane_id: c.plane_id,
                    world_width: c.world_width,
                    world_height: c.world_height,
                    camera_position: c.paint_camera_pos.map(|v| v.to_array()),
                    camera_target: c.paint_camera_target.map(|v| v.to_array()),
                    layers,
                })
            })
            .transpose()?;
        let targets = world.get_resource::<ProjectionTargets>();
        let projection = world
            .get::<ProjectionTarget>(entity)
            .map(|t| {
                let painting::MeshStorageMode::UvAtlas { resolution } = t.storage_mode else {
                    return Err("Project v1 does not support PTex projection targets".to_string());
                };
                Ok(ProjectionDocument {
                    resolution,
                    layers: targets
                        .map(|t| t.document_layers(entity))
                        .unwrap_or_default(),
                })
            })
            .transpose()?;
        let current = materials
            .get(
                &world
                    .get::<MeshMaterial3d<StandardMaterial>>(entity)
                    .ok_or("Missing or unsupported project material component")?
                    .0,
            )
            .ok_or("Missing project material asset")?;
        let mut material = if let Some(targets) = targets {
            targets.document_material(
                entity,
                &world
                    .get::<MeshMaterial3d<StandardMaterial>>(entity)
                    .ok_or("Missing or unsupported project material component")?
                    .0,
                current,
                materials,
            )?
        } else {
            current.clone()
        };
        #[cfg(feature = "mesh_painting")]
        let shared = crate::uv_layer_scene::capture(world, entity, current)?;
        #[cfg(feature = "mesh_painting")]
        let (uv_layers, mesh_uv) = if let Some((layers, original)) = shared {
            material = original;
            (Some(layers), None)
        } else {
            (None, crate::project_uv::capture(world, entity, &material)?)
        };
        #[cfg(feature = "mesh_painting")]
        let mesh_uv = mesh_uv.map(|(data, original)| {
            material = original;
            data
        });
        #[cfg(not(feature = "mesh_painting"))]
        let mesh_uv = None;
        #[cfg(not(feature = "mesh_painting"))]
        let uv_layers = None;
        if canvas.is_some() {
            if material.base_color != Color::WHITE {
                return Err("Canvas base color was externally edited; restore its white display material before saving.".into());
            }
            if let Some(texture) = world.get::<crate::CanvasTexture>(entity) {
                if material.base_color_texture.as_ref() != Some(&texture.image_handle) {
                    return Err(
                        "Canvas display texture no longer belongs to its layer pipeline".into(),
                    );
                }
            }
            material.base_color_texture = None;
        }
        #[cfg(feature = "selection")]
        let selectable = world.get::<crate::Selectable>(entity).map(|s| s.id.clone());
        #[cfg(not(feature = "selection"))]
        let selectable = None;
        #[cfg(feature = "mesh_painting")]
        let paintable = world
            .get::<crate::PaintableMesh>(entity)
            .map(|p| (p.mesh_id, StorageDocument::capture(p.storage_mode)));
        #[cfg(not(feature = "mesh_painting"))]
        let paintable = None;
        objects.push(ObjectDocument {
            id,
            name: world.get::<Name>(entity).map(|v| v.as_str().to_string()),
            selectable,
            translation: transform.translation.to_array(),
            rotation: transform.rotation.to_array(),
            scale: transform.scale.to_array(),
            hidden: world
                .get::<Visibility>(entity)
                .is_some_and(|v| *v == Visibility::Hidden),
            mesh,
            material: MaterialDocument::capture(&material, images)?,
            canvas,
            projection,
            paintable,
            mesh_uv,
            uv_layers,
            #[cfg(feature = "sculpting")]
            sculpt,
        });
    }
    objects.sort_by_key(|o| o.id);
    let next_plane = world
        .get_resource::<CanvasPlaneIdGenerator>()
        .map_or(0, CanvasPlaneIdGenerator::document_next_id)
        .max(
            objects
                .iter()
                .filter_map(|o| o.canvas.as_ref().map(|c| c.plane_id.saturating_add(1)))
                .max()
                .unwrap_or(0),
        );
    let next_mesh = objects
        .iter()
        .filter_map(|o| o.paintable.as_ref().map(|p| p.0.saturating_add(1)))
        .max()
        .unwrap_or(0);
    #[cfg(feature = "mesh_painting")]
    let next_mesh = next_mesh.max(
        world
            .get_resource::<crate::MeshIdGenerator>()
            .map_or(0, crate::MeshIdGenerator::document_next_id),
    );
    let active_canvas = world
        .get_resource::<crate::ActiveCanvasPlane>()
        .and_then(|a| a.entity)
        .and_then(|e| world.get::<CanvasPlane>(e))
        .map(|c| c.plane_id);
    #[cfg(feature = "sculpting")]
    let sculpt_brush = world
        .get_resource::<crate::SculptState>()
        .map(|s| (crate::sculpt_mode::sculpt_snapshot(s), s.brush_autosmooth));
    #[cfg(not(feature = "sculpting"))]
    let sculpt_brush = None;
    let document = ProjectDocument {
        format: "pentimento-project".into(),
        version: if objects.iter().any(|o| o.uv_layers.is_some()) {
            3
        } else {
            2
        },
        next_object_id: next,
        next_plane_id: next_plane,
        next_mesh_id: next_mesh,
        next_stroke_id: world
            .get_resource::<crate::StrokeIdGenerator>()
            .map_or(0, crate::StrokeIdGenerator::document_next_id),
        object_counter: crate::add_object::project_counter(world).max(
            objects
                .iter()
                .filter_map(|o| {
                    o.selectable
                        .as_ref()
                        .and_then(|s| s.strip_prefix("object_"))
                        .and_then(|s| s.parse::<u32>().ok())
                })
                .max()
                .unwrap_or(0),
        ),
        uv_receiver: world
            .get_resource::<crate::PaintMode>()
            .and_then(|p| p.direct_target)
            .and_then(|e| {
                world.get::<ProjectObjectId>(e).map(|id| id.0).or_else(|| {
                    assigned
                        .iter()
                        .find_map(|(entity, id)| (*entity == e).then_some(*id))
                })
            })
            .filter(|_| objects.iter().any(|o| o.uv_layers.is_some()))
            .filter(|id| {
                objects.iter().any(|o| {
                    o.id == *id
                        && matches!(o.paintable.as_ref(), Some((_, StorageDocument::Uv(..))))
                })
            }),
        objects,
        brush: BrushDocument {
            paint: painting.brush_preset.clone(),
            color: painting.brush_color,
            blend: match painting.blend_mode {
                painting::BlendMode::Normal => BlendMode::Normal,
                painting::BlendMode::Erase => BlendMode::Erase,
            },
            sculpt: sculpt_brush,
        },
        active_canvas,
        live_projection: world
            .get_resource::<crate::ProjectionMode>()
            .is_some_and(|p| p.live_projection),
        view: cameras.into_iter().next(),
        lighting: world
            .get_resource::<crate::SceneLighting>()
            .map(|s| s.settings.clone()),
        ambient_occlusion: world
            .get_resource::<crate::SceneAmbientOcclusion>()
            .map(|s| s.settings.clone()),
        mesh_brush,
    };
    document.validate()?;
    Ok((document, assigned))
}
impl ProjectDocument {
    fn validate(&self) -> Result<(), String> {
        if self.format != "pentimento-project" {
            return Err("Not a Pentimento project".into());
        }
        if self.version != 1 && self.version != 2 && self.version != 3 {
            return Err(format!(
                "Unsupported Pentimento project version {} (supported: 1,2,3)",
                self.version
            ));
        }
        if self.view.as_ref().is_some_and(|v| {
            !v.valid()
                || !v.restore().calculate_position().is_finite()
                || !Transform::from_translation(v.restore().calculate_position())
                    .looking_at(Vec3::from_array(v.target), Vec3::Y)
                    .compute_affine()
                    .is_finite()
        }) {
            return Err("Invalid project camera".into());
        }
        if let Some(l) = &self.lighting {
            if !l
                .sun_direction
                .iter()
                .chain(&l.sun_color)
                .chain(&l.ambient_color)
                .chain(&[
                    l.sun_intensity,
                    l.ambient_intensity,
                    l.time_of_day,
                    l.cloudiness,
                    l.moon_phase,
                    l.azimuth_angle,
                    l.pollution,
                ])
                .all(|v| v.is_finite())
                || !Vec3::from_array(l.sun_direction)
                    .length_squared()
                    .is_finite()
                || Vec3::from_array(l.sun_direction).length_squared() == 0.0
                || l.sun_intensity < 0.
                || l.ambient_intensity < 0.
                || !(0.0..=24.0).contains(&l.time_of_day)
                || ![l.cloudiness, l.moon_phase, l.pollution]
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v))
            {
                return Err("Invalid project lighting".into());
            }
        }
        if self.ambient_occlusion.as_ref().is_some_and(|a| {
            a.quality_level > 3
                || !a.constant_object_thickness.is_finite()
                || !(0.0625..=4.0).contains(&a.constant_object_thickness)
        }) {
            return Err("Invalid project ambient occlusion".into());
        }
        if self.objects.len() > 256
            || self.object_counter == u32::MAX
            || self.next_stroke_id == u64::MAX
            || self.next_object_id == u64::MAX
            || self.next_plane_id == u32::MAX
            || self.next_mesh_id == u32::MAX
        {
            return Err("Invalid or exhausted project counters/object limit".into());
        }
        if !crate::brush_presets::valid_project_brushes(
            &self.brush.paint,
            self.brush.color,
            self.brush.blend,
            self.brush.sculpt.as_ref().map(|(s, a)| (s, *a)),
        ) {
            return Err("Invalid project brush parameters".into());
        }
        #[cfg(not(feature = "sculpting"))]
        if self.brush.sculpt.is_some() {
            return Err("This build cannot open sculpt project data".into());
        }
        if let Some(brush) = &self.mesh_brush {
            if self.version < 2
                || !crate::brush_presets::valid_project_brushes(
                    &brush.preset,
                    brush.color,
                    brush.blend,
                    None,
                )
            {
                return Err("Invalid/version-incompatible DirectUV brush state".into());
            }
        }
        #[cfg(not(feature = "mesh_painting"))]
        if self.mesh_brush.is_some()
            || self
                .objects
                .iter()
                .any(|o| o.mesh_uv.is_some() || o.uv_layers.is_some())
        {
            return Err("This build cannot restore DirectUV paint data".into());
        }
        let mut ids = HashSet::new();
        let mut planes = HashSet::new();
        let mut mesh_ids = HashSet::new();
        let mut selections = HashSet::new();
        let mut pixels = 0usize;
        let mut records = 0usize;
        let mut image_bytes = 0usize;
        for o in &self.objects {
            if o.id == 0
                || o.id >= self.next_object_id
                || !ids.insert(o.id)
                || o.name.as_ref().is_some_and(|s| s.len() > 256)
                || o.selectable
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || s.len() > 256 || !selections.insert(s))
            {
                return Err("Invalid/duplicate project object identity".into());
            }
            let rotation = Quat::from_array(o.rotation);
            let affine = Transform {
                translation: Vec3::from_array(o.translation),
                rotation,
                scale: Vec3::from_array(o.scale),
            }
            .compute_affine();
            if !o
                .translation
                .iter()
                .chain(&o.rotation)
                .chain(&o.scale)
                .all(|v| v.is_finite())
                || (rotation.length_squared() - 1.0).abs() > 1e-4
                || !affine.is_finite()
                || !affine.matrix3.determinant().is_finite()
                || affine.matrix3.determinant() == 0.0
            {
                return Err("Invalid project transform".into());
            }
            if o.selectable
                .as_ref()
                .and_then(|s| s.strip_prefix("object_"))
                .and_then(|s| s.parse::<u32>().ok())
                .is_some_and(|n| n > self.object_counter)
            {
                return Err("Invalid selectable object allocation counter".into());
            }
            o.mesh.validate()?;
            o.material.validate()?;
            records = records.saturating_add(o.mesh.positions.len());
            image_bytes = image_bytes
                .saturating_add(o.material.texture.as_ref().map_or(0, |i| i.byte_count()));
            if let Some(c) = &o.canvas {
                c.layers.validate()?;
                if c.plane_id >= self.next_plane_id
                    || !planes.insert(c.plane_id)
                    || !c.world_width.is_finite()
                    || !c.world_height.is_finite()
                    || c.world_width <= 0.0
                    || c.world_height <= 0.0
                    || c.camera_position
                        .iter()
                        .chain(&c.camera_target)
                        .flatten()
                        .any(|v| !v.is_finite())
                {
                    return Err("Invalid project canvas identity/view".into());
                }
                pixels = pixels.saturating_add(
                    c.layers
                        .layers
                        .iter()
                        .map(|l| l.pixels.len())
                        .sum::<usize>(),
                );
            }
            if let Some((id, storage)) = &o.paintable {
                storage.validate()?;
                if *id >= self.next_mesh_id || !mesh_ids.insert(*id) {
                    return Err("Invalid project mesh paint identity".into());
                }
            }
            if let Some(layers) = &o.uv_layers {
                layers.validate()?;
                if self.version != 3
                    || o.mesh_uv.is_some()
                    || o.canvas.is_some()
                    || !o.mesh.has_uv0()
                    || self.mesh_brush.is_none()
                {
                    return Err("Version-incompatible or duplicate UV layer owner".into());
                }
                match &o.paintable {
                    Some((_, StorageDocument::Uv(w, h)))
                        if (*w, *h) == (layers.width, layers.height) => {}
                    _ => return Err("UV layer dimensions/storage disagree".into()),
                }
                if o.projection.as_ref().is_some_and(|p| {
                    !p.layers.is_empty() || p.resolution != (layers.width, layers.height)
                }) {
                    return Err("UV layer receiver has a competing projection owner".into());
                }
                pixels = pixels.saturating_add(layers.sample_count());
            }
            match (&o.paintable, &o.mesh_uv) {
                (Some((_, StorageDocument::Uv(w, h))), Some(uv)) if self.version >= 2 => {
                    uv.validate((*w, *h))?;
                    if self.mesh_brush.is_none() || o.canvas.is_some() || !o.mesh.has_uv0() {
                        return Err(
                            "Missing DirectUV brush/mesh UV0 or incompatible canvas owner".into(),
                        );
                    }
                    if uv.pixels.iter().flatten().any(|p| p.to_bits() != 0)
                        && o.projection.as_ref().is_some_and(|p| !p.layers.is_empty())
                    {
                        return Err(
                            "Simultaneous DirectUV/projection paint ownership is unsupported"
                                .into(),
                        );
                    }
                    pixels = pixels.saturating_add(uv.pixels.len());
                }
                (Some((_, StorageDocument::Uv(..))), None)
                    if self.version >= 2 && o.uv_layers.is_none() =>
                {
                    return Err("Missing required v2 DirectUV surface".into());
                }
                (_, Some(_)) => {
                    return Err("DirectUV surface requires v2 UV-atlas paintable ownership".into());
                }
                _ => {}
            }
            #[cfg(not(feature = "mesh_painting"))]
            if o.paintable.is_some() {
                return Err("This build cannot restore mesh painting components".into());
            }
            #[cfg(not(feature = "selection"))]
            if o.selectable.is_some() {
                return Err("This build cannot restore selectable objects".into());
            }
            #[cfg(feature = "sculpting")]
            if let Some(s) = &o.sculpt {
                records = records.saturating_add(s.record_count());
            }
        }
        if self.active_canvas.is_some_and(|p| !planes.contains(&p)) {
            return Err("Missing active project canvas".into());
        }
        for o in &self.objects {
            if let Some(p) = &o.projection {
                let (w, h) = p.resolution;
                let n = (w as usize)
                    .checked_mul(h as usize)
                    .ok_or("Projection size overflow")?;
                let mut layer_ids = HashSet::new();
                if w == 0 || h == 0 || w > 1048 || h > 1048 || p.layers.len() > 256 {
                    return Err("Invalid project projection dimensions/count".into());
                }
                pixels = pixels.saturating_add(n);
                for (plane, data) in &p.layers {
                    if !planes.contains(plane)
                        || !layer_ids.insert(*plane)
                        || data.len() != n
                        || data
                            .iter()
                            .flatten()
                            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                    {
                        return Err("Invalid/missing project projection layer or canvas".into());
                    }
                    pixels = pixels.saturating_add(data.len());
                }
            }
        }
        if self.uv_receiver.is_some_and(|id| {
            self.version != 3
                || !self.objects.iter().any(|o| {
                    o.id == id && matches!(o.paintable.as_ref(), Some((_, StorageDocument::Uv(..))))
                })
        }) {
            return Err("Missing/version-incompatible active UV receiver".into());
        }
        if pixels > MAX_PIXELS || records > MAX_RECORDS || image_bytes > 32 * 1024 * 1024 {
            return Err("Project decoded pixel/topology/image budget exceeded".into());
        }
        Ok(())
    }
    fn prepare(self) -> Result<PreparedProject, String> {
        self.validate()?;
        let mut painting = PaintingResource::new();
        painting.set_brush_preset(self.brush.paint.clone());
        painting.set_brush_color(self.brush.color);
        painting.set_blend_mode_ipc(self.brush.blend);
        let mut objects = Vec::new();
        for o in &self.objects {
            if let Some(c) = &o.canvas {
                painting.restore_document_layers(c.plane_id, c.layers.clone())?;
            }
            if let Some(uv) = &o.mesh_uv {
                let material = o.material.clone().restore(&mut Assets::<Image>::default());
                let c = material.base_color.to_linear();
                if [c.red, c.green, c.blue, c.alpha]
                    .iter()
                    .zip(&uv.original_linear_color)
                    .any(|(a, b)| a.to_bits() != b.to_bits())
                {
                    return Err("DirectUV baseline color disagrees with original material".into());
                }
                if let Some(image) = &o.material.texture {
                    image.validate_direct_uv(uv.width, uv.height)?;
                }
            }
            if let Some(layers) = &o.uv_layers {
                if let Some(image) = &o.material.texture {
                    image.validate_direct_uv(layers.width, layers.height)?;
                }
            }
            #[cfg(feature = "sculpting")]
            let sculpt = o.sculpt.clone().map(|s| s.restore()).transpose()?;
            #[cfg(feature = "sculpting")]
            if let Some(s) = &sculpt {
                let rendered =
                    MeshDocument::capture(&sculpting::merge_chunks(s).mesh.to_bevy_mesh())?;
                let matches = rendered.same_sculpt_surface(&o.mesh)
                    || painting::half_edge::HalfEdgeMesh::from_bevy_mesh_welded(
                        &o.mesh.clone().restore(),
                    )
                    .ok()
                    .and_then(|m| MeshDocument::capture(&m.to_bevy_mesh()).ok())
                    .is_some_and(|m| rendered.same_sculpt_surface(&m));
                if !matches {
                    return Err("Project sculpt geometry and render mesh disagree".into());
                }
            }
            objects.push(PreparedObject {
                document: o.clone(),
                mesh: o.mesh.clone().restore(),
                #[cfg(feature = "sculpting")]
                sculpt,
            });
        }
        Ok(PreparedProject {
            document: self,
            objects,
            painting,
        })
    }
}
fn clear_messages<T: Message>(world: &mut World) {
    if let Some(mut m) = world.get_resource_mut::<Messages<T>>() {
        m.clear();
    }
}
fn queued<T: Message>(world: &World) -> bool {
    world
        .get_resource::<Messages<T>>()
        .is_some_and(|m| !m.is_empty())
}
fn save_ready(world: &mut World) -> Result<(), String> {
    let pending = queued::<crate::AddObjectEvent>(world)
        || queued::<crate::CanvasPlaneEvent>(world)
        || queued::<crate::PaintEvent>(world)
        || queued::<crate::ProjectionEvent>(world)
        || queued::<crate::EditModeEvent>(world);
    #[cfg(feature = "sculpting")]
    let pending = pending || queued::<crate::SculptEvent>(world);
    #[cfg(feature = "mesh_editing")]
    let pending = pending || queued::<crate::MeshEditEvent>(world);
    #[cfg(feature = "mesh_painting")]
    let pending = pending || queued::<crate::MeshPaintEvent>(world);
    let projection_pending = !crate::projection_painting::document_projection_settled(world);
    if pending || projection_pending {
        Err("Wait for the scene and live projection to settle, then save again; no project file was changed.".into())
    } else {
        Ok(())
    }
}
fn install(world: &mut World, prepared: PreparedProject) -> Result<(), String> {
    idle(world)?;
    if !world.contains_resource::<Assets<Mesh>>()
        || !world.contains_resource::<Assets<Image>>()
        || !world.contains_resource::<Assets<StandardMaterial>>()
    {
        return Err("Project assets unavailable; document unchanged".into());
    }
    let old = document_entities(world);
    if prepared.document.view.is_some()
        && world
            .query_filtered::<Entity, With<crate::MainCamera>>()
            .iter(world)
            .count()
            != 1
    {
        return Err("Project requires exactly one main orbit camera in this editor".into());
    }
    if prepared.document.lighting.is_some() && !world.contains_resource::<crate::SceneLighting>()
        || prepared.document.ambient_occlusion.is_some()
            && !world.contains_resource::<crate::SceneAmbientOcclusion>()
    {
        return Err("Project lighting resources unavailable".into());
    }
    // No fallible operations follow. Asset handles/Entity IDs are recreated locally.
    if let Some(mut out) = world.get_resource_mut::<OutboundUiMessages>() {
        out.messages.clear();
    }
    for entity in old {
        world.despawn(entity);
    }
    #[cfg(feature = "sculpting")]
    if let Some(mut s) = world.get_resource_mut::<crate::sculpt_mode::SculptingData>() {
        let chunks = std::mem::take(&mut s.chunk_entities);
        *s = Default::default();
        for e in chunks {
            world.despawn(e);
        }
    }
    crate::add_object::restore_project_counter(world, prepared.document.object_counter);
    world.insert_resource(prepared.painting);
    world.insert_resource(ProjectionTargets::default());
    #[cfg(feature = "mesh_painting")]
    world.insert_resource(crate::projection_painting::PendingUvApplies::default());
    #[cfg(feature = "mesh_painting")]
    world.insert_resource(crate::projection_painting::LiveUvPreview::default());
    world.insert_resource(crate::MeshRaycastCache::default());
    if let Some(v) = &prepared.document.view {
        let e = world
            .query_filtered::<Entity, With<crate::MainCamera>>()
            .single(world)
            .expect("validated main camera");
        let orbit = v.restore();
        let transform = Transform::from_translation(orbit.calculate_position())
            .looking_at(orbit.target, Vec3::Y);
        world.entity_mut(e).insert((orbit, transform));
    }
    if let Some(l) = &prepared.document.lighting {
        world.resource_mut::<crate::SceneLighting>().settings = l.clone();
    }
    if let Some(a) = &prepared.document.ambient_occlusion {
        world
            .resource_mut::<crate::SceneAmbientOcclusion>()
            .update(a.clone());
    }
    world.insert_resource(CanvasPlaneIdGenerator::from_document(
        prepared.document.next_plane_id,
    ));
    world.insert_resource(crate::StrokeIdGenerator::from_document(
        prepared.document.next_stroke_id,
    ));
    world.insert_resource(crate::PaintMode::default());
    world.insert_resource(crate::EditModeState::default());
    world.insert_resource(crate::ActiveCanvasPlane::default());
    world.insert_resource(crate::GizmoState::default());
    if let Some(mut p) = world.get_resource_mut::<crate::FrontendScenePointerInput>() {
        p.clear();
    }
    #[cfg(feature = "selection")]
    world.insert_resource(crate::SelectionState::default());
    #[cfg(feature = "sculpting")]
    {
        world.insert_resource(crate::SculptState::default());
        if let Some((settings, autosmooth)) = &prepared.document.brush.sculpt {
            crate::sculpt_mode::restore_saved_brush(world, settings, *autosmooth)
                .expect("validated detached project brush");
        }
        clear_messages::<crate::SculptEvent>(world);
    }
    #[cfg(feature = "mesh_editing")]
    {
        world.insert_resource(crate::MeshEditState::default());
        clear_messages::<crate::MeshEditEvent>(world);
    }
    #[cfg(feature = "mesh_painting")]
    {
        world.insert_resource(crate::MeshPaintState::default());
        let mut mesh_painting = crate::MeshPaintingResource::default();
        if let Some(b) = &prepared.document.mesh_brush {
            mesh_painting.set_brush_preset(b.preset.clone());
            mesh_painting.set_brush_color(b.color);
            mesh_painting.set_blend_mode(match b.blend {
                BlendMode::Normal => painting::BlendMode::Normal,
                BlendMode::Erase => painting::BlendMode::Erase,
            });
        }
        world.insert_resource(mesh_painting);
        world.insert_resource(crate::MeshIdGenerator::from_document(
            prepared.document.next_mesh_id,
        ));
        clear_messages::<crate::MeshPaintEvent>(world);
    }
    clear_messages::<crate::PaintEvent>(world);
    clear_messages::<crate::CanvasPlaneEvent>(world);
    clear_messages::<crate::ProjectionEvent>(world);
    clear_messages::<crate::AddObjectEvent>(world);
    clear_messages::<crate::EditModeEvent>(world);
    clear_messages::<bevy::window::WindowEvent>(world);
    clear_messages::<bevy::window::CursorMoved>(world);
    clear_messages::<bevy::input::mouse::MouseMotion>(world);
    clear_messages::<bevy::input::mouse::MouseWheel>(world);
    clear_messages::<bevy::input::mouse::MouseButtonInput>(world);
    clear_messages::<bevy::input::keyboard::KeyboardInput>(world);
    if let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
        keys.reset_all();
    }
    if let Some(mut buttons) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
        buttons.reset_all();
    }
    if let Some(mut buffer) =
        world.get_resource_mut::<crate::painting_system::DirtyTileUploadBuffer>()
    {
        buffer.canvases.clear();
    }
    let mut active = None;
    for object in prepared.objects {
        let o = object.document;
        let material = o
            .material
            .restore(&mut world.resource_mut::<Assets<Image>>());
        let mh = world.resource_mut::<Assets<Mesh>>().add(object.mesh);
        let mat = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(material);
        let mut entity = world.spawn((
            ProjectObjectId(o.id),
            Mesh3d(mh),
            MeshMaterial3d(mat.clone()),
            Transform {
                translation: Vec3::from_array(o.translation),
                rotation: Quat::from_array(o.rotation),
                scale: Vec3::from_array(o.scale),
            },
            if o.hidden {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
        ));
        if let Some(name) = o.name {
            entity.insert(Name::new(name));
        }
        #[cfg(feature = "selection")]
        if let Some(id) = o.selectable {
            entity.insert(crate::Selectable { id });
        }
        #[cfg(feature = "mesh_painting")]
        if let Some((mesh_id, storage)) = &o.paintable {
            entity.insert(crate::PaintableMesh {
                mesh_id: *mesh_id,
                storage_mode: storage.restore(),
            });
        }
        #[cfg(feature = "mesh_painting")]
        if let Some(uv) = o.mesh_uv {
            let id = o.paintable.as_ref().unwrap().0;
            let entity_id = entity.id();
            crate::project_uv::install(world, entity_id, id, uv, mat.clone());
            entity = world.entity_mut(entity_id);
        }
        #[cfg(feature = "mesh_painting")]
        if let Some(layers) = o.uv_layers {
            let id = o.paintable.as_ref().unwrap().0;
            let entity_id = entity.id();
            crate::uv_layer_scene::install(world, entity_id, id, layers, mat.clone());
            entity = world.entity_mut(entity_id);
        }
        #[cfg(feature = "sculpting")]
        if let Some(mesh) = object.sculpt {
            entity.insert(ProjectSculptGeometry {
                mesh,
                rendered: o.mesh,
            });
        }
        if let Some(c) = o.canvas {
            let mut plane = CanvasPlane::new(
                c.plane_id,
                c.layers.width,
                c.layers.height,
                c.world_width,
                c.world_height,
            );
            plane.paint_camera_pos = c.camera_position.map(Vec3::from_array);
            plane.paint_camera_target = c.camera_target.map(Vec3::from_array);
            plane.active = prepared.document.active_canvas == Some(c.plane_id);
            if plane.active {
                active = Some(entity.id());
            }
            entity.insert(plane);
        }
        if let Some(p) = o.projection {
            entity.insert(ProjectionTarget::uv_atlas(p.resolution));
            let id = entity.id();
            world
                .resource_mut::<ProjectionTargets>()
                .restore_document_layers(id, p.resolution, p.layers);
        }
    }
    #[cfg(feature = "mesh_painting")]
    if let Some(id) = prepared.document.uv_receiver {
        let entity = world
            .query::<(Entity, &ProjectObjectId)>()
            .iter(world)
            .find_map(|(e, i)| (i.0 == id).then_some(e));
        if let Some(mut mode) = world.get_resource_mut::<crate::PaintMode>() {
            mode.direct_target = entity;
        }
    }
    world.resource_mut::<crate::ActiveCanvasPlane>().entity = active;
    world.insert_resource(crate::ProjectionMode::default());
    world.resource_mut::<ProjectState>().next_object_id = prepared.document.next_object_id;
    world.resource_mut::<ProjectState>().generation =
        world.resource::<ProjectState>().generation.wrapping_add(1);
    if let Some(mut out) = world.get_resource_mut::<OutboundUiMessages>() {
        out.send(BevyToUi::EditModeChanged {
            mode: pentimento_ipc::EditMode::None,
        });
        out.send(BevyToUi::SelectionChanged {
            selected_ids: Vec::new(),
        });
    }
    let layers = active
        .and_then(|e| world.get::<CanvasPlane>(e))
        .and_then(|c| {
            world
                .resource::<PaintingResource>()
                .get_pipeline(c.plane_id)
        })
        .map(|p| p.layers.layer_info())
        .unwrap_or_default()
        .into_iter()
        .map(|l| pentimento_ipc::LayerInfo {
            id: l.id,
            name: l.name,
            visible: l.visible,
            opacity: l.opacity,
            is_active: l.is_active,
        })
        .collect();
    if let Some(mut out) = world.get_resource_mut::<OutboundUiMessages>() {
        out.send(BevyToUi::LayerStateChanged { layers });
    }
    crate::brush_ui::send_brush_state(world);
    Ok(())
}
fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|e| format!("Cannot inspect project file: {e}"))?;
    if !metadata.is_file() {
        return Err("Project path is not a regular file".into());
    }
    let f = std::fs::File::open(path).map_err(|e| format!("Cannot open project: {e}"))?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Project path is not a regular file".into());
    }
    let mut bytes = Vec::new();
    f.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Project file exceeds 64 MiB".into());
    }
    Ok(bytes)
}
fn parse(bytes: &[u8]) -> Result<ProjectDocument, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Project file exceeds 64 MiB".into());
    }
    // Header probe gives a useful unsupported-version error before interpreting v1 fields.
    #[derive(Deserialize)]
    struct Header {
        format: String,
        version: u32,
    }
    let h: Header =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid project JSON: {e}"))?;
    if h.format != "pentimento-project" {
        return Err("Not a Pentimento project".into());
    }
    if h.version != 1 && h.version != 2 && h.version != 3 {
        return Err(format!(
            "Unsupported Pentimento project version {} (supported: 1,2,3)",
            h.version
        ));
    }
    let document: ProjectDocument =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid project schema: {e}"))?;
    document.validate()?;
    Ok(document)
}
struct LimitedWriter(Vec<u8>);
impl Write for LimitedWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(b.len()) as u64 > MAX_BYTES {
            return Err(std::io::Error::other("Project file exceeds 64 MiB"));
        }
        self.0.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn serialize(document: &ProjectDocument) -> Result<Vec<u8>, String> {
    document.validate()?;
    let mut w = LimitedWriter(Vec::new());
    serde_json::to_writer(&mut w, document).map_err(|e| e.to_string())?;
    Ok(w.0)
}
fn atomic_save(path: &Path, bytes: &[u8], expected: Option<&[u8]>) -> Result<(), String> {
    atomic_save_with(path, bytes, expected, || Ok(()))
}
fn atomic_save_with(
    path: &Path,
    bytes: &[u8],
    expected: Option<&[u8]>,
    before_commit: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !path.is_absolute() || path.extension().is_none_or(|e| e != "json") {
        return Err(
            "Use an absolute local path ending in .json (recommended: .pentimento.json).".into(),
        );
    }
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Project save refuses symbolic-link destinations".into());
    }
    let parent = path.parent().ok_or("Missing project directory")?;
    let name = path
        .file_name()
        .ok_or("Missing project filename")?
        .to_string_lossy();
    let lock_path = parent.join(format!(".{name}.lock"));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("Cannot lock project save: {e}"))?;
    lock.try_lock()
        .map_err(|e| format!("Project is being saved elsewhere: {e}"))?;
    let unchanged = || -> Result<(), String> {
        match (expected,read_bounded(path)){(Some(old),Ok(current)) if old==current=>Ok(()),(None,Err(_)) if !path.exists()=>Ok(()),_=>Err("Project changed outside this editor or destination already exists. Open it again or save to a new path.".into())}
    };
    unchanged()?;
    let permissions = if expected.is_some() {
        Some(
            std::fs::metadata(path)
                .map_err(|e| format!("Cannot read owned project permissions: {e}"))?
                .permissions(),
        )
    } else {
        None
    };
    let mut temp = None;
    for i in 0..32u32 {
        let p = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), i));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(f) => {
                temp = Some((p, f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("Cannot create project temporary file: {e}")),
        }
    }
    let (tmp, mut file) = temp.ok_or("Project temporary paths unavailable")?;
    let result = (|| {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)
                .map_err(|e| format!("Cannot preserve owned project permissions: {e}"))?;
        }
        file.write_all(bytes)
            .map_err(|e| format!("Project write failed: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Project sync failed: {e}"))?;
        before_commit()?;
        unchanged()?;
        std::fs::rename(&tmp, path).map_err(|e| format!("Project replacement failed: {e}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
pub(crate) fn send_state(world: &mut World) {
    world.init_resource::<ProjectState>();
    let active = idle(world).is_err();
    let s = world.resource::<ProjectState>();
    let message = BevyToUi::ProjectStateChanged {
        path: s.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        available: !cfg!(target_arch = "wasm32"),
        active,
        blocked: s.blocked,
        notice: s.notice.clone(),
    };
    if let Some(mut out) = world.get_resource_mut::<OutboundUiMessages>() {
        out.send(message);
    }
}
pub(crate) fn dispatch(world: &mut World, command: &ProjectCommand) {
    world.init_resource::<ProjectState>();
    if matches!(command, ProjectCommand::GetState) {
        send_state(world);
        return;
    }
    let result = (|| -> Result<(), String> {
        if cfg!(target_arch = "wasm32") {
            return Err("Local project file access is available in the native editor.".into());
        }
        idle(world)?;
        match command {
            ProjectCommand::Save { path } => {
                save_ready(world)?;
                let path = PathBuf::from(path);
                let expected = world
                    .resource::<ProjectState>()
                    .path
                    .as_ref()
                    .filter(|p| *p == &path)
                    .and_then(|_| world.resource::<ProjectState>().original.clone());
                if world.resource::<ProjectState>().blocked && expected.is_some() {
                    return Err(
                        "Reopen the externally changed project or save to a new path.".into(),
                    );
                }
                let (document, assigned) = capture(world)?;
                // Saving uses the same admission path as opening; no invalid editable geometry is published.
                document.clone().prepare()?;
                let bytes = serialize(&document)?;
                if let Err(e) = atomic_save(&path, &bytes, expected.as_deref()) {
                    if e.contains("outside this editor") {
                        world.resource_mut::<ProjectState>().blocked = true;
                    }
                    return Err(e);
                }
                for (entity, id) in assigned {
                    world.entity_mut(entity).insert(ProjectObjectId(id));
                }
                let mut s = world.resource_mut::<ProjectState>();
                s.path = Some(path);
                s.original = Some(bytes);
                s.next_object_id = document.next_object_id;
                s.blocked = false;
                s.notice = Some(
                    "Project saved losslessly. Undo history is local to this editing session."
                        .into(),
                );
            }
            ProjectCommand::Open { path } => {
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err("Use an absolute local project path.".into());
                }
                let bytes = read_bounded(&path)?;
                let document = parse(&bytes)?;
                let was_live = document.live_projection;
                let prepared = document.prepare()?;
                install(world, prepared)?;
                let mut s = world.resource_mut::<ProjectState>();
                s.path = Some(path);
                s.original = Some(bytes);
                s.blocked = false;
                s.notice=Some(if was_live{"Project opened. Live projection is paused; resume it to project again. Undo history starts fresh."}else{"Project opened. Undo history starts fresh."}.into());
            }
            ProjectCommand::GetState => {}
        }
        Ok(())
    })();
    let success = result.is_ok();
    if let Err(message) = result {
        world.resource_mut::<ProjectState>().notice = Some(message.clone());
        if let Some(mut out) = world.get_resource_mut::<OutboundUiMessages>() {
            out.send(BevyToUi::Error {
                code: "project_operation_failed".into(),
                message,
            });
        }
    }
    let message = world
        .resource::<ProjectState>()
        .notice
        .clone()
        .unwrap_or_default();
    if let Some(mut out) = world.get_resource_mut::<OutboundUiMessages>() {
        out.send(BevyToUi::ProjectOperationFinished {
            operation: if matches!(command, ProjectCommand::Open { .. }) {
                "Open"
            } else {
                "Save"
            }
            .into(),
            success,
            message,
        });
    }
    send_state(world);
}
#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;

#[cfg(all(test, feature = "mesh_painting"))]
#[path = "project_uv_tests.rs"]
mod direct_uv_tests;
