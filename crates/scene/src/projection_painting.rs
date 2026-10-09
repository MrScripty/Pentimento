//! Canvas-to-UV projection. Each canvas owns a replaceable projection layer, so
//! live edits, erase, cancel, and undo never accumulate the same paint twice.
//! Target texels sample the source canvas through visible mesh geometry. This
//! inverse mapping avoids holes when an atlas is larger than the canvas image.

use bevy::asset::{AssetEventSystems, RenderAssetUsages};
use bevy::camera::visibility::VisibilitySystems;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::pbr::UvChannel;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, Face, PrimitiveTopology, TextureDimension, TextureFormat,
};
use bevy::transform::TransformSystems;
use std::collections::{BTreeMap, HashMap};

use painting::{
    MeshStorageMode,
    projection_target::{ProjectionTargetStorage, UvAtlasTarget},
    raycast::{MeshRaycastData, TriangleHit, ray_triangle_intersection},
};

use crate::camera::MainCamera;
use crate::canvas_plane::{ActiveCanvasPlane, CanvasPlane};
use crate::painting_system::{CanvasTexture, PaintingResource};
use crate::projection_mode::{ProjectionEvent, ProjectionMode, ProjectionTarget};

#[derive(Resource, Default)]
pub struct ProjectionTargets {
    targets: HashMap<Entity, UvAtlasTarget>,
    textures: HashMap<Entity, Handle<Image>>,
    // Ascending plane IDs preserve canvas creation/layer order.
    layers: BTreeMap<u32, HashMap<Entity, Vec<[f32; 4]>>>,
    sessions: HashMap<u32, ProjectionSession>,
    appearances: HashMap<Entity, ProjectionAppearance>,
}

impl ProjectionTargets {
    pub(crate) fn document_material(
        &self,
        entity: Entity,
        handle: &Handle<StandardMaterial>,
        current: &StandardMaterial,
        materials: &Assets<StandardMaterial>,
    ) -> Result<StandardMaterial, String> {
        let Some(a) = self
            .appearances
            .get(&entity)
            .filter(|a| a.painted_handle.as_ref() == Some(handle))
        else {
            return Ok(current.clone());
        };
        if current.base_color != Color::WHITE
            || current.base_color_texture.as_ref() != self.textures.get(&entity)
        {
            return Err("Projected receiver base appearance was edited outside projection; clear/reapply projection before saving.".into());
        }
        let original = materials
            .get(&a.original_handle)
            .ok_or("Missing original projection material asset")?;
        let mut document = current.clone();
        document.base_color = original.base_color;
        document.base_color_texture = original.base_color_texture.clone();
        Ok(document)
    }
    #[cfg(feature = "mesh_painting")]
    pub(crate) fn validate_layer_migration(
        &self,
        world: &World,
        entity: Entity,
    ) -> Result<(), String> {
        let appearance = self
            .appearances
            .get(&entity)
            .ok_or("Missing acknowledged projection owner")?;
        if appearance.migration_conflicted {
            return Err("Projection display had an external edit before a partial upload; reopen its owned project".into());
        }
        let handle = world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .ok_or("Missing projection material")?;
        if appearance.painted_handle.as_ref() != Some(&handle.0) {
            return Err("The receiver is not owned by its published projection".into());
        }
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(&handle.0)
            .ok_or("Missing projection material asset")?;
        if material.base_color != Color::WHITE
            || material.base_color_texture.as_ref() != self.textures.get(&entity)
        {
            return Err("Projection material was edited externally".into());
        }
        let images = world.resource::<Assets<Image>>();
        let target = self.get(entity).ok_or("Missing projection atlas")?;
        let resolution = target.resolution();
        let image = self
            .get_texture(entity)
            .and_then(|h| images.get(h))
            .ok_or("Missing projection display")?;
        crate::project_assets::ImageDocument::capture(image)?
            .validate_direct_uv(resolution.0, resolution.1)?;
        if image.data.as_ref() != appearance.published_bytes.as_ref()
            || appearance.published_bytes.is_none()
        {
            return Err("Projection display is pending or externally changed".into());
        }
        if appearance
            .original
            .base_color_texture
            .as_ref()
            .and_then(|h| images.get(h))
            .and_then(|i| i.data.as_ref())
            != appearance.original_bytes.as_ref()
        {
            return Err("Projection original image was edited externally".into());
        }
        let mut expected = vec![[0.; 4]; resolution.0 as usize * resolution.1 as usize];
        for layer in self
            .layers
            .values()
            .filter_map(|layers| layers.get(&entity))
        {
            for (dst, src) in expected.iter_mut().zip(layer) {
                *dst = over(*src, *dst);
            }
        }
        if !painting::uv_layers::same_uv_pixels(&expected, target.surface().surface().pixels()) {
            return Err("Projection atlas changed outside its raw source layers".into());
        }
        Ok(())
    }
    /// Borrow the exact mapped source pixels retained by a legacy Canvas layer.
    pub fn projection_layer_pixels(&self, plane_id: u32, entity: Entity) -> Option<&[[f32; 4]]> {
        self.layers.get(&plane_id)?.get(&entity).map(Vec::as_slice)
    }
    pub(crate) fn document_layers(&self, entity: Entity) -> Vec<(u32, Vec<[f32; 4]>)> {
        self.layers
            .iter()
            .filter_map(|(&plane, layers)| {
                layers.get(&entity).map(|pixels| (plane, pixels.clone()))
            })
            .collect()
    }
    pub(crate) fn restore_document_layers(
        &mut self,
        entity: Entity,
        resolution: (u32, u32),
        layers: Vec<(u32, Vec<[f32; 4]>)>,
    ) {
        self.get_or_create(entity, resolution);
        for (plane, pixels) in layers {
            self.layers.entry(plane).or_default().insert(entity, pixels);
        }
        self.composite_layers();
    }
    pub fn get_or_create(&mut self, entity: Entity, resolution: (u32, u32)) -> &mut UvAtlasTarget {
        self.targets.entry(entity).or_insert_with(|| {
            let mut target = UvAtlasTarget::new(resolution.0, resolution.1);
            // The first image must include unpainted original-material texels.
            target.clear([0.0; 4]);
            target
        })
    }
    pub fn get(&self, entity: Entity) -> Option<&UvAtlasTarget> {
        self.targets.get(&entity)
    }
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut UvAtlasTarget> {
        self.targets.get_mut(&entity)
    }
    pub fn set_texture(&mut self, entity: Entity, handle: Handle<Image>) {
        self.textures.insert(entity, handle);
    }
    pub fn get_texture(&self, entity: Entity) -> Option<&Handle<Image>> {
        self.textures.get(&entity)
    }
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &UvAtlasTarget)> {
        self.targets.iter().map(|(e, t)| (*e, t))
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut UvAtlasTarget)> {
        self.targets.iter_mut().map(|(e, t)| (*e, t))
    }

    fn composite_layers(&mut self) {
        for (entity, target) in &mut self.targets {
            let count = target.surface().surface().pixel_count();
            let mut pixels = vec![[0.0; 4]; count];
            for layer in self.layers.values().filter_map(|layers| layers.get(entity)) {
                for (dst, src) in pixels.iter_mut().zip(layer) {
                    *dst = over(*src, *dst);
                }
            }
            target.replace_pixels(&pixels);
        }
    }

    #[cfg(feature = "mesh_painting")]
    pub(crate) fn detach_shared(&mut self, entity: Entity, resolution: (u32, u32)) {
        for layers in self.layers.values_mut() {
            layers.remove(&entity);
        }
        self.appearances.remove(&entity);
        self.targets.remove(&entity);
        self.get_or_create(entity, resolution);
        self.sessions.clear();
    }
    fn clear(&mut self, entity: Option<Entity>) {
        for layer in self.layers.values_mut() {
            if let Some(entity) = entity {
                layer.remove(&entity);
            } else {
                layer.clear();
            }
        }
        // Keep source snapshots: a clear stays clear until the user edits or
        // explicitly projects again, even while live projection is enabled.
        self.composite_layers();
    }
}

struct ProjectionAppearance {
    original_handle: Handle<StandardMaterial>,
    original: StandardMaterial,
    painted_handle: Option<Handle<StandardMaterial>>,
    warned_unreadable: bool,
    migration_conflicted: bool,
    published_bytes: Option<Vec<u8>>,
    original_bytes: Option<Vec<u8>>,
}

#[derive(Resource, Default)]
pub struct MeshRaycastCache {
    cache: HashMap<Entity, MeshRaycastData>,
    revision: u64,
}
impl MeshRaycastCache {
    pub fn get_or_build(
        &mut self,
        entity: Entity,
        mesh: &Mesh,
        _transform: &GlobalTransform,
    ) -> Option<&MeshRaycastData> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.cache.entry(entity) {
            entry.insert(extract_mesh_raycast_data(mesh)?);
        }
        self.cache.get(&entity)
    }
    pub fn invalidate(&mut self, entity: Entity) {
        self.cache.remove(&entity);
        self.revision = self.revision.wrapping_add(1);
    }
}

fn extract_mesh_raycast_data(mesh: &Mesh) -> Option<MeshRaycastData> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let positions: Vec<Vec3> = positions.iter().copied().map(Vec3::from).collect();
    if positions.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let indices: Vec<u32> = match mesh.indices() {
        Some(Indices::U16(i)) => i.iter().map(|i| *i as u32).collect(),
        Some(Indices::U32(i)) => i.clone(),
        None => (0..positions.len() as u32).collect(),
    };
    if !indices.len().is_multiple_of(3) || indices.iter().any(|i| *i as usize >= positions.len()) {
        return None;
    }
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) if n.len() == positions.len() => {
            n.iter().copied().map(Vec3::from).collect()
        }
        _ => {
            let mut normals = vec![Vec3::ZERO; positions.len()];
            for tri in indices.chunks_exact(3) {
                let normal = (positions[tri[1] as usize] - positions[tri[0] as usize])
                    .cross(positions[tri[2] as usize] - positions[tri[0] as usize]);
                for &index in tri {
                    normals[index as usize] += normal;
                }
            }
            normals.into_iter().map(Vec3::normalize_or_zero).collect()
        }
    };
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(uv)) if uv.len() == positions.len() => {
            uv.iter().copied().map(Vec2::from).collect()
        }
        _ => Vec::new(),
    };
    Some(MeshRaycastData {
        positions,
        indices,
        normals,
        uvs,
        tangents: Vec::new(),
    })
}

/// A world-space triangle for both texel coverage and depth testing.
struct ProjectionTriangle {
    entity: Entity,
    positions: [Vec3; 3],
    normals: [Vec3; 3],
    uvs: Option<[Vec2; 3]>,
    cull_mode: Option<Face>,
}
impl ProjectionTriangle {
    fn from_mesh(
        entity: Entity,
        mesh: &MeshRaycastData,
        face: usize,
        transform: &GlobalTransform,
        cull_mode: Option<Face>,
    ) -> Option<Self> {
        let matrix = transform.affine();
        let determinant = matrix.matrix3.determinant();
        if !determinant.is_finite() || determinant.abs() < 1e-10 {
            return None;
        }
        // Normals must use the inverse transpose, especially under nonuniform scale.
        let normal_matrix = matrix.matrix3.inverse().transpose();
        let ids = mesh.triangle_indices(face);
        let ids = [ids.0 as usize, ids.1 as usize, ids.2 as usize];
        let positions = ids.map(|i| matrix.transform_point3(mesh.positions[i]));
        if positions.iter().any(|p| !p.is_finite()) {
            return None;
        }
        Some(Self {
            entity,
            positions,
            normals: ids.map(|i| (normal_matrix * mesh.normals[i]).normalize_or_zero()),
            uvs: (!mesh.uvs.is_empty()).then(|| ids.map(|i| mesh.uvs[i])),
            cull_mode,
        })
    }
    fn visible_from(&self, camera: Vec3, invert_culling: bool) -> bool {
        // Match rasterization winding, never interpolated shading normals.
        // World-space vertices include mirrored object transforms automatically.
        let [a, b, c] = self.positions;
        let facing = (b - a).cross(c - a).dot(camera - a);
        if !facing.is_finite() || facing == 0.0 {
            return false;
        }
        let front = (facing > 0.0) != invert_culling;
        match self.cull_mode {
            Some(Face::Back) => front,
            Some(Face::Front) => !front,
            None => true,
        }
    }

    fn center(&self) -> Vec3 {
        (self.positions[0] + self.positions[1] + self.positions[2]) / 3.0
    }
    fn normal(&self, weights: Vec3) -> Vec3 {
        (self.normals[0] * weights.x + self.normals[1] * weights.y + self.normals[2] * weights.z)
            .normalize_or_zero()
    }
}

// A small, projection-local BVH. Building it once per geometry change avoids
// testing every scene triangle for every atlas texel during live painting.
struct RayNode {
    min: Vec3,
    max: Vec3,
    children: Option<(Box<RayNode>, Box<RayNode>)>,
    indices: Vec<usize>,
}
impl RayNode {
    fn build(triangles: &[ProjectionTriangle], mut indices: Vec<usize>) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for &i in &indices {
            for p in triangles[i].positions {
                min = min.min(p);
                max = max.max(p);
            }
        }
        let children = if indices.len() > 8 {
            let extent = max - min;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            indices.sort_unstable_by(|a, b| {
                triangles[*a].center()[axis].total_cmp(&triangles[*b].center()[axis])
            });
            let right = indices.split_off(indices.len() / 2);
            let left = std::mem::take(&mut indices);
            Some((
                Box::new(Self::build(triangles, left)),
                Box::new(Self::build(triangles, right)),
            ))
        } else {
            None
        };
        Self {
            min,
            max,
            children,
            indices,
        }
    }
    fn intersects(&self, origin: Vec3, direction: Vec3, max_t: f32) -> bool {
        let mut near: f32 = 0.0;
        let mut far = max_t;
        for axis in 0..3 {
            if direction[axis].abs() < 1e-12 {
                if origin[axis] < self.min[axis] - 1e-5 || origin[axis] > self.max[axis] + 1e-5 {
                    return false;
                }
            } else {
                let a = (self.min[axis] - origin[axis]) / direction[axis];
                let b = (self.max[axis] - origin[axis]) / direction[axis];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far + 1e-5 {
                    return false;
                }
            }
        }
        true
    }
    fn nearest(
        &self,
        triangles: &[ProjectionTriangle],
        origin: Vec3,
        direction: Vec3,
        best: &mut Option<(usize, TriangleHit)>,
    ) {
        if !self.intersects(
            origin,
            direction,
            best.as_ref().map_or(f32::INFINITY, |(_, hit)| hit.t),
        ) {
            return;
        }
        if let Some((left, right)) = &self.children {
            left.nearest(triangles, origin, direction, best);
            right.nearest(triangles, origin, direction, best);
        }
        for &index in &self.indices {
            let p = triangles[index].positions;
            if let Some(hit) = ray_triangle_intersection(origin, direction, p[0], p[1], p[2])
                && best.as_ref().is_none_or(|(_, old)| hit.t < old.t)
            {
                *best = Some((index, hit));
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct ProjectionContext {
    camera: Vec3,
    invert_culling: bool,
    world_to_canvas: Mat4,
    size: Vec2,
    resolution: UVec2,
}
#[derive(Clone, PartialEq)]
struct MeshStamp {
    entity: Entity,
    mesh: AssetId<Mesh>,
    transform: Mat4,
    visible: bool,
    cull_mode: Option<Face>,
    resolution: Option<(u32, u32)>,
}
#[cfg(feature = "mesh_painting")]
#[derive(Resource, Default)]
pub(crate) struct PendingUvApplies(Vec<std::sync::Arc<UvApplyAdmission>>);
#[cfg(feature = "mesh_painting")]
struct UvApplyAdmission {
    source: Entity,
    receiver: Entity,
    layer: u32,
    mesh_tick: bevy::ecs::change_detection::Tick,
    context: ProjectionContext,
    geometry: Vec<MeshStamp>,
    pixels: Vec<[f32; 4]>,
}
#[cfg(feature = "mesh_painting")]
impl PendingUvApplies {
    pub(crate) fn bytes(&self) -> usize {
        self.0
            .iter()
            .enumerate()
            .filter(|(i, v)| !self.0[..*i].iter().any(|p| std::sync::Arc::ptr_eq(p, v)))
            .map(|(_, v)| v.pixels.len() * 16 + v.geometry.len() * std::mem::size_of::<MeshStamp>())
            .sum()
    }
}
#[cfg(feature = "mesh_painting")]
#[derive(Resource, Default)]
pub(crate) struct LiveUvPreview {
    admission: Option<std::sync::Arc<UvApplyAdmission>>,
    rendered: bool,
}
#[cfg(feature = "mesh_painting")]
impl LiveUvPreview {
    pub(crate) fn bytes(&self) -> usize {
        self.admission.as_ref().map_or(0, |a| {
            a.pixels.len() * 16 + a.geometry.len() * std::mem::size_of::<MeshStamp>()
        })
    }
    pub(crate) fn unshared_bytes(&self, pending: Option<&PendingUvApplies>) -> usize {
        if self.admission.as_ref().is_some_and(|a| {
            pending.is_some_and(|p| p.0.iter().any(|v| std::sync::Arc::ptr_eq(v, a)))
        }) {
            0
        } else {
            self.bytes()
        }
    }
    pub(crate) fn active(&self) -> bool {
        self.admission.is_some()
    }
}
#[cfg(feature = "mesh_painting")]
pub(crate) fn begin_uv_preview(world: &mut World) -> Result<(), String> {
    if crate::brush_presets::active(world)
        || world
            .get_resource::<crate::FrontendScenePointerInput>()
            .is_some_and(|p| p.has_scene_press())
    {
        return Err("Finish or cancel the source stroke before live UV preview".into());
    }
    if world.resource::<crate::PaintMode>().target != pentimento_ipc::PaintTarget::Canvas {
        return Err("Choose Canvas projection before live UV preview".into());
    }
    if world
        .get_resource::<LiveUvPreview>()
        .is_some_and(|p| p.active())
    {
        return Ok(());
    }
    if world
        .get_resource::<PendingUvApplies>()
        .is_some_and(|p| !p.0.is_empty())
    {
        return Err("Wait for pending UV Apply".into());
    }
    crate::mesh_painting_system::sync_mesh_paint_owners(world);
    admit_uv_apply(world)?;
    let admission = world.resource_mut::<PendingUvApplies>().0.pop().unwrap();
    let id = world
        .get::<crate::PaintableMesh>(admission.receiver)
        .ok_or("Missing receiver")?
        .mesh_id;
    world
        .resource_mut::<crate::MeshPaintingResource>()
        .begin_projection_preview(id)?;
    world.insert_resource(LiveUvPreview {
        admission: Some(admission),
        rendered: false,
    });
    let mut mode = world.resource_mut::<ProjectionMode>();
    mode.live_projection = true;
    mode.enabled = true;
    world.resource_mut::<crate::PaintMode>().target_notice=Some("Live UV preview is staged on the selected layer. Apply commits once and pauses live; Cancel preview discards it while retaining Canvas edits.".into());
    Ok(())
}
#[cfg(feature = "mesh_painting")]
pub(crate) fn cancel_uv_preview(world: &mut World) {
    if !world
        .get_resource::<LiveUvPreview>()
        .is_some_and(|p| p.active())
    {
        return;
    }
    // UI can run after the ordinary Update owner guard. Recheck exact current
    // image bytes synchronously before scheduling any baseline rollback.
    crate::mesh_painting_system::sync_mesh_paint_owners(world);
    world
        .resource_mut::<crate::MeshPaintingResource>()
        .cancel_projection_preview();
    world.insert_resource(LiveUvPreview::default());
    if let Some(mut p) = world.get_resource_mut::<PendingUvApplies>() {
        p.0.clear();
    }
    if let Some(mut events) = world.get_resource_mut::<Messages<ProjectionEvent>>() {
        events.clear();
    }
    if let Some(mut mode) = world.get_resource_mut::<ProjectionMode>() {
        mode.live_projection = false;
        mode.enabled = false;
    }
    world.resource_mut::<crate::PaintMode>().target_notice =
        Some("UV preview cancelled. Canvas edits retained.".into());
}
#[cfg(feature = "mesh_painting")]
fn stop_uv_preview(
    shared: &mut crate::MeshPaintingResource,
    live: &mut LiveUvPreview,
    mode: &mut ProjectionMode,
) {
    shared.cancel_projection_preview();
    *live = LiveUvPreview::default();
    mode.live_projection = false;
    mode.enabled = false;
}
#[cfg(feature = "mesh_painting")]
pub(crate) fn admit_uv_apply(world: &mut World) -> Result<(), String> {
    if !world.contains_resource::<bevy::ecs::message::Messages<ProjectionEvent>>() {
        return Err("Canvas projection is unavailable in this editor build".into());
    }
    let mesh_tick = world
        .get_resource_ref::<Assets<Mesh>>()
        .ok_or("Mesh assets unavailable")?
        .last_changed();
    let receiver = world
        .resource::<crate::PaintMode>()
        .direct_target
        .ok_or("Select a UV receiver")?;
    let id = world
        .get::<crate::PaintableMesh>(receiver)
        .ok_or("Missing UV receiver")?
        .mesh_id;
    let layer = world
        .resource::<crate::MeshPaintingResource>()
        .uv_layers(id)
        .ok_or("Enable UV layers")?
        .document()
        .active_layer;
    let source = world
        .resource::<ActiveCanvasPlane>()
        .entity
        .ok_or("Choose a source canvas")?;
    let plane = world
        .get::<CanvasPlane>(source)
        .ok_or("Missing source canvas")?;
    let plane_id = plane.plane_id;
    let canvas = world
        .get::<GlobalTransform>(source)
        .ok_or("Wait for canvas transform settlement")?;
    let context_base = (
        canvas.to_matrix().inverse(),
        Vec2::new(plane.world_width, plane.world_height),
        UVec2::new(plane.width, plane.height),
        plane.paint_camera_pos,
    );
    let (camera, invert_culling) = world
        .query_filtered::<(&GlobalTransform, Option<&Camera>), With<MainCamera>>()
        .single(world)
        .map(|(t, c)| (t.translation(), c.is_some_and(|c| c.invert_culling)))
        .map_err(|_| "Wait for the main camera")?;
    let context = ProjectionContext {
        camera: context_base.3.unwrap_or(camera),
        invert_culling,
        world_to_canvas: context_base.0,
        size: context_base.1,
        resolution: context_base.2,
    };
    let mut geometry: Vec<_> = world
        .query_filtered::<(
            Entity,
            &Mesh3d,
            &GlobalTransform,
            Option<&ProjectionTarget>,
            Option<&InheritedVisibility>,
            &MeshMaterial3d<StandardMaterial>,
        ), Without<CanvasPlane>>()
        .iter(world)
        .filter_map(|(entity, mesh, t, target, visible, m)| {
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&m.0)
                .map(|m| MeshStamp {
                    entity,
                    mesh: mesh.0.id(),
                    transform: t.to_matrix(),
                    visible: visible.is_none_or(|v| v.get()),
                    cull_mode: m.cull_mode,
                    resolution: target.and_then(|t| match t.storage_mode {
                        MeshStorageMode::UvAtlas { resolution } => Some(resolution),
                        _ => None,
                    }),
                })
        })
        .collect();
    geometry.sort_unstable_by_key(|g| g.entity.to_bits());
    if let Some(pin) = world
        .get_resource::<LiveUvPreview>()
        .and_then(|p| p.admission.as_ref())
    {
        if pin.source != source
            || pin.receiver != receiver
            || pin.layer != layer
            || pin.mesh_tick != mesh_tick
            || pin.context != context
            || pin.geometry != geometry
        {
            return Err("UV preview mapping changed; cancel preview before retrying".into());
        }
    }
    if let Some(previous) = world
        .get_resource::<PendingUvApplies>()
        .and_then(|p| p.0.last())
        .cloned()
    {
        if previous.source != source
            || previous.receiver != receiver
            || previous.layer != layer
            || previous.mesh_tick != mesh_tick
            || previous.context != context
            || previous.geometry != geometry
        {
            return Err("Wait for pending Apply before changing its source or mapping".into());
        }
        let mut pending = world.resource_mut::<PendingUvApplies>();
        if pending.0.len() >= 128 {
            return Err("Pending UV Apply request limit reached".into());
        }
        pending.0.push(previous);
        return Ok(());
    }
    let mut painting = world.resource_mut::<PaintingResource>();
    let pipeline = painting
        .get_pipeline_mut(plane_id)
        .ok_or("Wait for source canvas setup")?;
    pipeline.layers.composite();
    let source_pixels = pipeline.layers.composited_surface().surface().pixels();
    if source_pixels.len() * 16 + geometry.len() * std::mem::size_of::<MeshStamp>()
        > painting::uv_layers::UV_PENDING_BYTES
    {
        return Err("UV Apply snapshot exceeds the 32 MiB pending payload limit".into());
    }
    let pixels = source_pixels.to_vec();
    let admission = UvApplyAdmission {
        source,
        receiver,
        layer,
        mesh_tick,
        context,
        geometry,
        pixels,
    };
    if admission.pixels.len() * 16 + admission.geometry.len() * std::mem::size_of::<MeshStamp>()
        > painting::uv_layers::UV_PENDING_BYTES
    {
        return Err("UV Apply snapshot exceeds the 32 MiB pending payload limit".into());
    }
    let admission = std::sync::Arc::new(admission);
    // Apply and live provenance share this bounded source snapshot. Replace
    // the older live snapshot only after its pinned mapping validates.
    if let Some(mut live) = world.get_resource_mut::<LiveUvPreview>() {
        if live.active() {
            live.admission = Some(admission.clone());
        }
    }
    world.init_resource::<PendingUvApplies>();
    world.resource_mut::<PendingUvApplies>().0.push(admission);
    Ok(())
}

struct ProjectionSession {
    context: ProjectionContext,
    geometry: Vec<MeshStamp>,
    revision: u64,
    mapping: HashMap<Entity, Vec<Option<usize>>>,
    source: Vec<[f32; 4]>,
}

// Apply runs after Update uploads. A plugin/external edit may happen later in
// that frame, so validate exact current owners again at the commit boundary.
#[cfg(feature = "mesh_painting")]
fn validate_pending_uv_owners(world: &mut World) {
    if world
        .get_resource::<LiveUvPreview>()
        .is_some_and(|p| p.active())
        || world
            .get_resource::<PendingUvApplies>()
            .is_some_and(|p| p.bytes() > 0)
    {
        crate::mesh_painting_system::sync_mesh_paint_owners(world);
    }
}

/// Save may arrive in Update before transform propagation and live projection.
/// Compare authoring inputs to the existing mapping snapshot without replaying it.
pub(crate) fn document_projection_settled(world: &mut World) -> bool {
    if !world
        .get_resource::<ProjectionMode>()
        .is_some_and(|m| m.live_projection)
    {
        return true;
    }
    let Some(entity) = world
        .get_resource::<ActiveCanvasPlane>()
        .and_then(|a| a.entity)
    else {
        return true;
    };
    let Some(plane) = world.get::<CanvasPlane>(entity) else {
        return false;
    };
    let plane_id = plane.plane_id;
    let Some(transform) = world.get::<Transform>(entity) else {
        return false;
    };
    if world.get::<ChildOf>(entity).is_some() {
        return false;
    }
    let mut context = ProjectionContext {
        camera: plane.paint_camera_pos.unwrap_or(Vec3::ZERO),
        invert_culling: false,
        world_to_canvas: GlobalTransform::from(*transform).to_matrix().inverse(),
        size: Vec2::new(plane.world_width, plane.world_height),
        resolution: UVec2::new(plane.width, plane.height),
    };
    let pinned_camera = plane.paint_camera_pos.is_some();
    let mut camera_query = world.query_filtered::<(
        &GlobalTransform,
        Option<&crate::OrbitCamera>,
        Option<&Camera>,
    ), With<MainCamera>>();
    let Ok((camera, orbit, view)) = camera_query.single(world) else {
        return false;
    };
    if !pinned_camera {
        context.camera = orbit.map_or_else(|| camera.translation(), |o| o.calculate_position());
    }
    context.invert_culling = view.is_some_and(|v| v.invert_culling);
    let mut query = world.query_filtered::<(
        Entity,
        &Mesh3d,
        &GlobalTransform,
        Option<&Transform>,
        Option<&ChildOf>,
        Option<&ProjectionTarget>,
        Option<&InheritedVisibility>,
        Option<&Visibility>,
        &MeshMaterial3d<StandardMaterial>,
    ), Without<CanvasPlane>>();
    let Some(materials) = world.get_resource::<Assets<StandardMaterial>>() else {
        return false;
    };
    let mut geometry = Vec::new();
    for (entity, mesh, global, local, parent, target, inherited, visibility, material) in
        query.iter(world)
    {
        // Project v1 rejects parented authoring objects rather than using stale globals.
        if parent.is_some() {
            return false;
        }
        let Some(material) = materials.get(&material.0) else {
            continue;
        };
        geometry.push(MeshStamp {
            entity,
            mesh: mesh.0.id(),
            transform: local.map_or_else(
                || global.to_matrix(),
                |t| GlobalTransform::from(*t).to_matrix(),
            ),
            visible: visibility.map_or_else(
                || inherited.is_none_or(|v| v.get()),
                |v| *v != Visibility::Hidden,
            ),
            cull_mode: material.cull_mode,
            resolution: target.and_then(|t| match t.storage_mode {
                MeshStorageMode::UvAtlas { resolution } => Some(resolution),
                _ => None,
            }),
        });
    }
    geometry.sort_unstable_by_key(|stamp| stamp.entity.to_bits());
    // Asset edits can precede publication of their Modified messages. Compare
    // the actual mapping inputs with the cache, including UV-less occluders.
    // Deleted receivers are absent here; their retained removal messages do not
    // indefinitely block an otherwise completed, empty mapping.
    let Some(meshes) = world.get_resource::<Assets<Mesh>>() else {
        return false;
    };
    let Some(cache) = world.get_resource::<MeshRaycastCache>() else {
        return false;
    };
    for stamp in geometry.iter().filter(|stamp| stamp.visible) {
        let current = meshes.get(stamp.mesh).and_then(extract_mesh_raycast_data);
        match (current, cache.cache.get(&stamp.entity)) {
            (Some(current), Some(saved))
                if current.positions == saved.positions
                    && current.indices == saved.indices
                    && current.uvs == saved.uvs => {}
            (None, None) => {}
            _ => return false,
        }
    }
    let Some(pipeline) = world
        .get_resource::<PaintingResource>()
        .and_then(|p| p.get_pipeline(plane_id))
    else {
        return false;
    };
    let source = pipeline.layers.document_composite_pixels();
    let revision = world
        .get_resource::<MeshRaycastCache>()
        .map_or(0, |c| c.revision);
    world
        .get_resource::<ProjectionTargets>()
        .and_then(|t| t.sessions.get(&plane_id))
        .is_some_and(|s| {
            s.source == source
                && s.context == context
                && s.geometry == geometry
                && s.revision == revision
        })
}

/// Project a world point back through the camera onto the canvas. UV y=0 is
/// the top image row; target mesh UV0 uses that same image-row convention.
fn source_pixel(point: Vec3, context: &ProjectionContext) -> Option<usize> {
    if context.resolution.min_element() == 0 || context.size.min_element() <= 0.0 {
        return None;
    }
    let inverse = context.world_to_canvas;
    if !inverse.is_finite() {
        return None;
    }
    let origin = inverse.transform_point3(context.camera);
    let endpoint = inverse.transform_point3(point);
    let direction = endpoint - origin;
    if direction.z.abs() < 1e-8 {
        return None;
    }
    let t = -origin.z / direction.z;
    // Geometry must be beyond the projection canvas, not between it and the camera.
    if t <= 0.0 || t > 1.00001 {
        return None;
    }
    let local = origin + direction * t;
    let uv = Vec2::new(
        local.x / context.size.x + 0.5,
        0.5 - local.y / context.size.y,
    );
    if !uv.is_finite() || uv.min_element() < 0.0 || uv.max_element() >= 1.0 {
        return None;
    }
    let x = (uv.x * context.resolution.x as f32) as u32;
    let y = (uv.y * context.resolution.y as f32) as u32;
    Some((y * context.resolution.x + x) as usize)
}

fn barycentric_uv(point: Vec2, triangle: [Vec2; 3]) -> Option<Vec3> {
    let a = triangle[1] - triangle[0];
    let b = triangle[2] - triangle[0];
    let p = point - triangle[0];
    let determinant = a.perp_dot(b);
    if !determinant.is_finite() || determinant.abs() < 1e-10 {
        return None;
    }
    let v = p.perp_dot(b) / determinant;
    let w = a.perp_dot(p) / determinant;
    let weights = Vec3::new(1.0 - v - w, v, w);
    (weights.min_element() >= -1e-5).then_some(weights)
}

fn build_mapping(
    context: &ProjectionContext,
    triangles: &[ProjectionTriangle],
    resolutions: &HashMap<Entity, (u32, u32)>,
) -> HashMap<Entity, Vec<Option<usize>>> {
    let mut mapping: HashMap<Entity, Vec<Option<usize>>> = resolutions
        .iter()
        .map(|(&entity, &(w, h))| (entity, vec![None; (w * h) as usize]))
        .collect();
    // Cull receivers and occluders identically. An invisible reverse-wound
    // face must neither receive paint nor hide a visible surface behind it.
    let visible: Vec<_> = (0..triangles.len())
        .filter(|&index| triangles[index].visible_from(context.camera, context.invert_culling))
        .collect();
    if visible.is_empty() {
        return mapping;
    }
    let tree = RayNode::build(triangles, visible.clone());
    for index in visible {
        let triangle = &triangles[index];
        let Some(&resolution) = resolutions.get(&triangle.entity) else {
            continue;
        };
        let Some(uvs) = triangle.uvs else {
            continue;
        };
        if uvs.iter().any(|uv| !uv.is_finite()) {
            continue;
        }
        let min = uvs[0].min(uvs[1]).min(uvs[2]).max(Vec2::ZERO);
        let max = uvs[0].max(uvs[1]).max(uvs[2]).min(Vec2::ONE);
        let width = resolution.0;
        let height = resolution.1;
        let x0 = (min.x * width as f32).floor().max(0.0) as u32;
        let y0 = (min.y * height as f32).floor().max(0.0) as u32;
        let x1 = ((max.x * width as f32).ceil() as u32).min(width);
        let y1 = ((max.y * height as f32).ceil() as u32).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                let uv = Vec2::new(
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                );
                let Some(weights) = barycentric_uv(uv, uvs) else {
                    continue;
                };
                let point = triangle.positions[0] * weights.x
                    + triangle.positions[1] * weights.y
                    + triangle.positions[2] * weights.z;
                if !triangle.normal(weights).is_finite() {
                    continue;
                }
                let Some(source) = source_pixel(point, context) else {
                    continue;
                };
                let distance = point.distance(context.camera);
                let direction = (point - context.camera).normalize_or_zero();
                let mut nearest = None;
                tree.nearest(triangles, context.camera, direction, &mut nearest);
                if let Some((index, hit)) = nearest {
                    // Same entity alone is insufficient: a back face can be occluded
                    // by a front face of that same mesh.
                    if triangles[index].entity == triangle.entity
                        && (hit.t - distance).abs() <= 1e-4 * distance.max(1.0)
                    {
                        mapping.get_mut(&triangle.entity).unwrap()[(y * width + x) as usize] =
                            Some(source);
                    }
                }
            }
        }
    }
    mapping
}

/// Straight-alpha source-over, also correct for translucent original materials.
fn over(src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
    let alpha = src[3] + dst[3] * (1.0 - src[3]);
    if alpha <= 0.0 {
        return [0.0; 4];
    }
    [
        (src[0] * src[3] + dst[0] * dst[3] * (1.0 - src[3])) / alpha,
        (src[1] * src[3] + dst[1] * dst[3] * (1.0 - src[3])) / alpha,
        (src[2] * src[3] + dst[2] * dst[3] * (1.0 - src[3])) / alpha,
        alpha,
    ]
}

pub struct ProjectionPaintingPlugin;
impl Plugin for ProjectionPaintingPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "mesh_painting")]
        app.init_resource::<LiveUvPreview>();
        app.init_resource::<ProjectionTargets>()
            .init_resource::<MeshRaycastCache>()
            .add_systems(
                PostUpdate,
                (
                    register_projection_targets,
                    setup_projection_textures,
                    invalidate_mesh_cache,
                    #[cfg(feature = "mesh_painting")]
                    validate_pending_uv_owners,
                    live_projection_system,
                    upload_projection_textures,
                    projection_native_inspection,
                )
                    .chain()
                    .after(TransformSystems::Propagate)
                    .after(VisibilitySystems::VisibilityPropagate)
                    .after(AssetEventSystems),
            );
    }
}

type NewProjectionMeshes<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Mesh3d),
    (
        Without<ProjectionTarget>,
        Without<CanvasPlane>,
        With<MeshMaterial3d<StandardMaterial>>,
    ),
>;

fn register_projection_targets(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    query: NewProjectionMeshes,
) {
    for (entity, handle) in &query {
        if let Some(mesh) = meshes.get(&handle.0)
            && mesh.primitive_topology() == PrimitiveTopology::TriangleList
            && matches!(mesh.attribute(Mesh::ATTRIBUTE_UV_0), Some(VertexAttributeValues::Float32x2(uvs)) if !uvs.is_empty())
        {
            commands
                .entity(entity)
                .insert(ProjectionTarget::uv_atlas((512, 512)));
        }
    }
}

fn setup_projection_textures(
    mut images: ResMut<Assets<Image>>,
    materials: Res<Assets<StandardMaterial>>,
    mut targets: ResMut<ProjectionTargets>,
    #[cfg(feature = "mesh_painting")] mesh_paint: Option<Res<crate::MeshPaintingResource>>,
    #[cfg(feature = "mesh_painting")] mesh_textures: Query<&crate::MeshPaintTexture>,
    mut query: Query<(
        Entity,
        &mut ProjectionTarget,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (entity, mut component, material) in &mut query {
        #[cfg(feature = "mesh_painting")]
        if mesh_paint
            .as_ref()
            .is_some_and(|r| r.shared_id_for_entity(entity).is_some())
        {
            if let Ok(texture) = mesh_textures.get(entity) {
                if let MeshStorageMode::UvAtlas { resolution } = component.storage_mode {
                    targets.get_or_create(entity, resolution);
                    targets.set_texture(entity, texture.image_handle.clone());
                    component.texture_handle = Some(texture.image_handle.clone());
                }
            }
            continue;
        }
        if targets.get_texture(entity).is_some() {
            continue;
        }
        let MeshStorageMode::UvAtlas {
            resolution: (width, height),
        } = component.storage_mode
        else {
            if component.is_added() {
                warn!(
                    "PTex projection is unsupported for {entity:?}; use a mesh with UV0 and a UV atlas target"
                );
            }
            continue;
        };
        if width == 0 || height == 0 {
            continue;
        }
        let Some(original) = materials.get(&material.0) else {
            continue;
        };
        if original.base_color_channel != UvChannel::Uv0
            || original.uv_transform != bevy::math::Affine2::IDENTITY
        {
            if component.is_added() {
                warn!(
                    "Projection for {entity:?} requires untransformed UV0; original material remains unchanged"
                );
            }
            continue;
        }
        let image = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        let handle = images.add(image);
        targets.get_or_create(entity, (width, height));
        targets.set_texture(entity, handle.clone());
        targets.appearances.insert(
            entity,
            ProjectionAppearance {
                original_handle: material.0.clone(),
                original: original.clone(),
                painted_handle: None,
                warned_unreadable: false,
                migration_conflicted: false,
                published_bytes: images.get(&handle).and_then(|i| i.data.clone()),
                original_bytes: original
                    .base_color_texture
                    .as_ref()
                    .and_then(|h| images.get(h))
                    .and_then(|i| i.data.clone()),
            },
        );
        component.texture_handle = Some(handle);
    }
}

fn invalidate_mesh_cache(
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut cache: ResMut<MeshRaycastCache>,
) {
    if events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
        )
    }) {
        cache.cache.clear();
        cache.revision = cache.revision.wrapping_add(1);
    }
}

type ProjectionMeshQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Mesh3d,
        &'static GlobalTransform,
        Option<&'static ProjectionTarget>,
        Option<&'static InheritedVisibility>,
        &'static MeshMaterial3d<StandardMaterial>,
    ),
    (Without<CanvasPlane>, With<MeshMaterial3d<StandardMaterial>>),
>;

#[derive(SystemParam)]
struct ProjectionScene<'w, 's> {
    active: Res<'w, ActiveCanvasPlane>,
    canvases: Query<
        'w,
        's,
        (
            &'static CanvasPlane,
            &'static GlobalTransform,
            Option<&'static CanvasTexture>,
        ),
    >,
    mesh_query: ProjectionMeshQuery<'w, 's>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<StandardMaterial>>,
    camera: Query<'w, 's, (&'static GlobalTransform, Option<&'static Camera>), With<MainCamera>>,
}

fn live_projection_system(
    #[cfg(feature = "mesh_painting")] mut shared: Option<ResMut<crate::MeshPaintingResource>>,
    #[cfg(feature = "mesh_painting")] mut paint_mode: Option<ResMut<crate::PaintMode>>,
    #[cfg(feature = "mesh_painting")] mut outbound: Option<ResMut<crate::OutboundUiMessages>>,
    #[cfg(feature = "mesh_painting")] mut live_preview: Option<ResMut<LiveUvPreview>>,
    #[cfg(feature = "mesh_painting")] pending_applies: Option<ResMut<PendingUvApplies>>,
    window_events: Option<Res<Messages<bevy::window::WindowEvent>>>,
    primary_window: Query<(Entity, &Window), With<bevy::window::PrimaryWindow>>,
    mut window_cursor: Local<bevy::ecs::message::MessageCursor<bevy::window::WindowEvent>>,
    mut events: MessageReader<ProjectionEvent>,
    mut projection_mode: ResMut<ProjectionMode>,
    mut painting: ResMut<PaintingResource>,
    scene: ProjectionScene,
    mut targets: ResMut<ProjectionTargets>,
    mut cache: ResMut<MeshRaycastCache>,
) {
    let ProjectionScene {
        active,
        canvases,
        mesh_query,
        meshes,
        materials,
        camera,
    } = scene;
    #[cfg(feature = "mesh_painting")]
    let admissions = pending_applies
        .map(|mut p| std::mem::take(&mut p.0))
        .unwrap_or_default();
    let mut explicitly_requested = false;
    let mut explicit_count = 0usize;
    let mut enabling_requested = false;
    let mut trailing_clears = Vec::new();
    for event in events.read() {
        match event {
            ProjectionEvent::ProjectToScene => {
                explicitly_requested = true;
                explicit_count += 1;
                trailing_clears.clear();
            }
            ProjectionEvent::SetLiveProjection { enabled } => {
                // UI commands can arrive after the Update mode handler. Apply
                // the final toggle here too, before deciding whether to project.
                projection_mode.live_projection = *enabled;
                projection_mode.enabled = *enabled;
                enabling_requested = *enabled;
                if *enabled {
                    trailing_clears.clear();
                }
            }
            ProjectionEvent::ClearProjection { mesh_entity } => {
                targets.clear(Some(*mesh_entity));
                trailing_clears.push(Some(*mesh_entity));
            }
            ProjectionEvent::ClearAllProjections => {
                targets.clear(None);
                trailing_clears.push(None);
            }
        }
    }
    #[cfg(feature = "mesh_painting")]
    let shared_target = paint_mode
        .as_ref()
        .and_then(|p| p.direct_target)
        .and_then(|e| {
            shared
                .as_ref()
                .and_then(|r| r.shared_id_for_entity(e))
                .map(|id| (e, id))
        });
    #[cfg(feature = "mesh_painting")]
    let preview_active = live_preview.as_ref().is_some_and(|p| p.active());
    // Consume both message buffers every frame: native events arrive before
    // PreUpdate rotates them, unlike UI projection commands written in Update.
    let focus_lost = window_events.as_ref().is_some_and(|events| {
        window_cursor.read(events).fold(false, |lost, event| {
            let current = match event {
                bevy::window::WindowEvent::KeyboardFocusLost(_) => true,
                bevy::window::WindowEvent::WindowFocused(e) => {
                    !e.focused && primary_window.iter().any(|(w, _)| w == e.window)
                }
                _ => false,
            };
            lost || current
        })
    }) || primary_window.iter().any(|(_, w)| !w.focused);
    #[cfg(feature = "mesh_painting")]
    if preview_active {
        let pin = live_preview.as_ref().unwrap().admission.as_ref().unwrap();
        if focus_lost
            || Some(pin.source) != active.entity
            || Some(pin.receiver) != shared_target.map(|p| p.0)
            || !shared.as_ref().is_some_and(|r| r.preview_valid())
            || paint_mode
                .as_ref()
                .is_none_or(|p| !p.active || p.target != pentimento_ipc::PaintTarget::Canvas)
            || !projection_mode.live_projection
            || !canvases.contains(pin.source)
        {
            stop_uv_preview(
                shared.as_mut().unwrap(),
                live_preview.as_mut().unwrap(),
                &mut projection_mode,
            );
            if let Some(p) = paint_mode.as_mut() {
                p.target_notice =
                    Some("UV preview aborted without a commit. Canvas edits retained.".into());
            }
            if let Some(out) = outbound.as_mut() {
                out.send(pentimento_ipc::BevyToUi::Error {
                    code: "uv_preview_rejected".into(),
                    message:
                        "UV preview source or owner changed; preview aborted without committing"
                            .into(),
                });
            }
            return;
        }
    }
    #[cfg(feature = "mesh_painting")]
    if shared_target.is_some() && !preview_active {
        projection_mode.live_projection = false;
        enabling_requested = false;
    }
    #[cfg(feature = "mesh_painting")]
    if !admissions.is_empty()
        && (admissions.iter().any(|a| {
            Some(a.source) != active.entity
                || Some(a.receiver) != paint_mode.as_ref().and_then(|p| p.direct_target)
        }))
    {
        if let Some(out) = outbound.as_mut() {
            out.send(pentimento_ipc::BevyToUi::Error {code:"uv_projection_rejected".into(),message:"UV Apply source or receiver changed before commit; retry after scene settlement".into()});
        }
        return;
    }
    let requested = explicitly_requested || enabling_requested;
    #[cfg(feature = "mesh_painting")]
    let requested = requested || (preview_active && !live_preview.as_ref().unwrap().rendered);
    if !requested && !projection_mode.live_projection {
        return;
    }
    let Some(entity) = active.entity else {
        return;
    };
    #[cfg(feature = "mesh_painting")]
    let entity_of_source = entity;
    let Ok((plane, transform, canvas_texture)) = canvases.get(entity) else {
        return;
    };
    let Ok((camera, view)) = camera.single() else {
        return;
    };
    #[cfg(feature = "mesh_painting")]
    let source_active = painting.has_active_stroke();
    let Some(pipeline) = painting.get_pipeline_mut(plane.plane_id) else {
        return;
    };
    // The canvas upload composites in Update. Only repeat it for sources that
    // have no CanvasTexture (headless callers), or edits after that upload.
    if canvas_texture.is_none() || pipeline.has_dirty_tiles() {
        pipeline.layers.composite();
    }
    let pixels = pipeline.layers.composited_surface().surface().pixels();
    let context = ProjectionContext {
        // Keep a painted canvas tied to its saved projection view when the
        // user unlocks/orbits the scene to inspect the result.
        camera: plane.paint_camera_pos.unwrap_or(camera.translation()),
        invert_culling: view.is_some_and(|camera| camera.invert_culling),
        world_to_canvas: transform.to_matrix().inverse(),
        size: Vec2::new(plane.world_width, plane.world_height),
        resolution: UVec2::new(plane.width, plane.height),
    };
    let mut geometry: Vec<MeshStamp> = mesh_query
        .iter()
        .filter_map(|(entity, mesh, transform, target, visibility, material)| {
            let material = materials.get(&material.0)?;
            Some(MeshStamp {
                entity,
                mesh: mesh.0.id(),
                transform: transform.to_matrix(),
                visible: visibility.is_none_or(|v| v.get()),
                cull_mode: material.cull_mode,
                resolution: target.and_then(|target| match target.storage_mode {
                    MeshStorageMode::UvAtlas { resolution } => Some(resolution),
                    _ => None,
                }),
            })
        })
        .collect();
    geometry.sort_unstable_by_key(|stamp| stamp.entity.to_bits());
    #[cfg(feature = "mesh_painting")]
    if preview_active {
        let pin = live_preview.as_ref().unwrap().admission.as_ref().unwrap();
        if pin.context != context
            || pin.geometry != geometry
            || pin.mesh_tick != meshes.last_changed()
        {
            stop_uv_preview(
                shared.as_mut().unwrap(),
                live_preview.as_mut().unwrap(),
                &mut projection_mode,
            );
            if let Some(p) = paint_mode.as_mut() {
                p.target_notice =
                    Some("UV preview aborted without a commit. Canvas edits retained.".into());
            }
            if let Some(out) = outbound.as_mut() {
                out.send(pentimento_ipc::BevyToUi::Error {
                    code: "uv_preview_rejected".into(),
                    message:
                        "UV preview mapping or geometry changed; preview aborted without committing"
                            .into(),
                });
            }
            return;
        }
    }
    let mapping_changed = targets.sessions.get(&plane.plane_id).is_none_or(|session| {
        session.context != context
            || session.geometry != geometry
            || session.revision != cache.revision
    });
    if !requested
        && !mapping_changed
        && targets
            .sessions
            .get(&plane.plane_id)
            .is_some_and(|session| session.source == pixels)
    {
        return;
    }
    if mapping_changed {
        let mut triangles = Vec::new();
        let mut resolutions = HashMap::new();
        for stamp in &geometry {
            if !stamp.visible {
                continue;
            }
            let Some(mesh) = meshes.get(stamp.mesh) else {
                continue;
            };
            // Handles can be swapped without an AssetEvent::Modified.
            cache.cache.remove(&stamp.entity);
            let global = GlobalTransform::from(stamp.transform);
            let Some(data) = cache.get_or_build(stamp.entity, mesh, &global) else {
                continue;
            };
            for face in 0..data.triangle_count() {
                if let Some(triangle) = ProjectionTriangle::from_mesh(
                    stamp.entity,
                    data,
                    face,
                    &global,
                    stamp.cull_mode,
                ) {
                    triangles.push(triangle);
                }
            }
            if let Some(resolution) = stamp.resolution
                && targets.get(stamp.entity).is_some()
            {
                resolutions.insert(stamp.entity, resolution);
            }
        }
        let mapping = build_mapping(&context, &triangles, &resolutions);
        targets.sessions.insert(
            plane.plane_id,
            ProjectionSession {
                context: context.clone(),
                geometry: geometry.clone(),
                revision: cache.revision,
                mapping,
                source: Vec::new(),
            },
        );
    }
    let session = targets.sessions.get_mut(&plane.plane_id).unwrap();
    session.source = pixels.to_vec();
    let layer: HashMap<Entity, Vec<[f32; 4]>> = session
        .mapping
        .iter()
        .map(|(&entity, map)| {
            (
                entity,
                map.iter()
                    .map(|index| {
                        index
                            .and_then(|i| pixels.get(i).copied())
                            .unwrap_or([0.0; 4])
                    })
                    .collect(),
            )
        })
        .collect();
    #[cfg(feature = "mesh_painting")]
    if let Some((entity, id)) = shared_target {
        if (source_active && explicitly_requested)
            || shared.as_ref().is_some_and(|r| r.has_active_stroke())
        {
            if let Some(out) = outbound.as_mut() {
                out.send(pentimento_ipc::BevyToUi::Error {
                    code: "uv_projection_rejected".into(),
                    message: "Finish or cancel painting before UV Apply".into(),
                });
            }
            return;
        }
        if preview_active && !explicitly_requested {
            let result = layer
                .get(&entity)
                .ok_or_else(|| {
                    "The selected UV receiver has no visible projection coverage".to_string()
                })
                .and_then(|pixels| {
                    shared
                        .as_mut()
                        .unwrap()
                        .update_projection_preview(id, pixels)
                });
            match result {
                Ok(()) => live_preview.as_mut().unwrap().rendered = true,
                Err(message) => {
                    stop_uv_preview(
                        shared.as_mut().unwrap(),
                        live_preview.as_mut().unwrap(),
                        &mut projection_mode,
                    );
                    if let Some(out) = outbound.as_mut() {
                        out.send(pentimento_ipc::BevyToUi::Error {
                            code: "uv_preview_rejected".into(),
                            message,
                        });
                    }
                }
            }
            return;
        }
        if admissions.len() != explicit_count
            || admissions.iter().any(|a| {
                a.source != entity_of_source
                    || a.receiver != entity
                    || a.layer
                        != shared
                            .as_ref()
                            .unwrap()
                            .uv_layers(id)
                            .unwrap()
                            .document()
                            .active_layer
                    || a.mesh_tick != meshes.last_changed()
                    || a.context != context
                    || a.geometry != geometry
                    || !painting::uv_layers::same_uv_pixels(&a.pixels, pixels)
            })
        {
            if let Some(out) = outbound.as_mut() {
                out.send(pentimento_ipc::BevyToUi::Error {code:"uv_projection_rejected".into(),message:"UV Apply source or mapping changed before commit; retry after scene settlement".into()});
            }
            return;
        }
        if preview_active {
            let result = layer
                .get(&entity)
                .ok_or_else(|| {
                    "The selected UV receiver has no visible projection coverage".to_string()
                })
                .and_then(|pixels| {
                    shared
                        .as_mut()
                        .unwrap()
                        .update_projection_preview(id, pixels)
                })
                .and_then(|_| shared.as_mut().unwrap().commit_projection_preview());
            stop_uv_preview(
                shared.as_mut().unwrap(),
                live_preview.as_mut().unwrap(),
                &mut projection_mode,
            );
            if let Some(p) = paint_mode.as_mut() {
                p.target_notice=Some(match &result {
                    Ok(true)=>"UV preview applied once. Live is paused; enable it for the next staged edit.".into(),
                    Ok(false)=>"UV preview matched the layer; no edit recorded. Live is paused.".into(),
                    Err(message)=>message.clone(),
                });
            }
            if let Err(message) = result {
                if let Some(out) = outbound.as_mut() {
                    out.send(pentimento_ipc::BevyToUi::Error {
                        code: "uv_projection_rejected".into(),
                        message,
                    });
                }
            }
            return;
        }
        for _ in 0..explicit_count {
            let result = layer
                .get(&entity)
                .ok_or_else(|| {
                    "The selected UV receiver has no visible projection coverage".to_string()
                })
                .and_then(|pixels| shared.as_mut().unwrap().project_uv_layer(id, pixels));
            if let Err(message) = result {
                if let Some(p) = paint_mode.as_mut() {
                    p.target_notice = Some(message.clone());
                }
                if let Some(out) = outbound.as_mut() {
                    out.send(pentimento_ipc::BevyToUi::Error {
                        code: "uv_projection_rejected".into(),
                        message,
                    });
                }
            }
        }
        return;
    }
    #[cfg(feature = "mesh_painting")]
    let layer = layer
        .into_iter()
        .filter(|(e, _)| {
            shared
                .as_ref()
                .is_none_or(|r| r.shared_id_for_entity(*e).is_none())
        })
        .collect();
    targets.layers.insert(plane.plane_id, layer);
    targets.composite_layers();
    // A Clear received after Project in the same frame must remain the final
    // action, even if that frame also changed source pixels in live mode.
    for entity in trailing_clears {
        targets.clear(entity);
    }
}

fn srgb_to_linear(v: u8) -> f32 {
    let v = v as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let v = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round() as u8
}

fn original_pixel(
    material: &StandardMaterial,
    images: &Assets<Image>,
    uv: Vec2,
) -> Option<[f32; 4]> {
    let color = material.base_color.to_linear();
    let mut result = [color.red, color.green, color.blue, color.alpha];
    if let Some(handle) = &material.base_color_texture {
        let image = images.get(handle)?;
        let format = image.texture_descriptor.format;
        if !matches!(
            format,
            TextureFormat::Rgba8UnormSrgb
                | TextureFormat::Rgba8Unorm
                | TextureFormat::Bgra8UnormSrgb
                | TextureFormat::Bgra8Unorm
        ) {
            return None;
        }
        let data = image.data.as_ref()?;
        if data.len() < image.width() as usize * image.height() as usize * 4 {
            return None;
        }
        let x = ((uv.x * image.width() as f32) as u32).min(image.width().checked_sub(1)?);
        let y = ((uv.y * image.height() as f32) as u32).min(image.height().checked_sub(1)?);
        let pixel = data.get(
            ((y * image.width() + x) * 4) as usize..((y * image.width() + x + 1) * 4) as usize,
        )?;
        let channels = if matches!(
            format,
            TextureFormat::Bgra8UnormSrgb | TextureFormat::Bgra8Unorm
        ) {
            [2, 1, 0]
        } else {
            [0, 1, 2]
        };
        for i in 0..3 {
            result[i] *= if format.is_srgb() {
                srgb_to_linear(pixel[channels[i]])
            } else {
                pixel[channels[i]] as f32 / 255.0
            };
        }
        result[3] *= pixel[3] as f32 / 255.0;
    }
    Some(result)
}

fn upload_projection_textures(
    #[cfg(feature = "mesh_painting")] shared: Option<Res<crate::MeshPaintingResource>>,
    mut targets: ResMut<ProjectionTargets>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: Query<(
        Entity,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut ProjectionTarget,
    )>,
) {
    for (entity, mut handle, mut component) in &mut query {
        #[cfg(feature = "mesh_painting")]
        if shared
            .as_ref()
            .is_some_and(|r| r.shared_id_for_entity(entity).is_some())
        {
            continue;
        }
        let Some(target) = targets.get(entity) else {
            continue;
        };
        if !target.has_dirty_regions() {
            continue;
        }
        let Some(appearance) = targets.appearances.get(&entity) else {
            continue;
        };
        let original = appearance.original.clone();
        let resolution = target.resolution();
        let has_paint = target
            .surface()
            .surface()
            .pixels()
            .iter()
            .any(|p| p[3] > 0.0);
        let foreign = images
            .get(self::ProjectionTargets::get_texture(&targets, entity).unwrap())
            .and_then(|i| i.data.as_ref())
            != appearance.published_bytes.as_ref();
        let original_handle = appearance.original_handle.clone();
        if foreign {
            targets
                .appearances
                .get_mut(&entity)
                .unwrap()
                .migration_conflicted = true;
        }
        if !has_paint {
            handle.0 = original_handle;
        }
        // Keep unsupported/GPU-only base textures unchanged, with dirty paint
        // retained for retry when the original image becomes CPU-readable.
        if original_pixel(&original, &images, Vec2::splat(0.5)).is_none() {
            let appearance = targets.appearances.get_mut(&entity).unwrap();
            if has_paint && !appearance.warned_unreadable {
                warn!(
                    "Projection for {entity:?} is waiting for a CPU-readable RGBA8/BGRA8 base texture"
                );
                appearance.warned_unreadable = true;
            }
            continue;
        }
        targets
            .appearances
            .get_mut(&entity)
            .unwrap()
            .warned_unreadable = false;
        let texture = targets.get_texture(entity).unwrap().clone();
        let regions = targets.get_mut(entity).unwrap().take_dirty_regions();
        // Update only changed CPU image rectangles. Bevy manages reliable Image
        // extraction/upload, including images that were not GPU-ready this frame.
        for region in regions {
            let mut data = Vec::with_capacity((region.size.0 * region.size.1 * 4) as usize);
            for y in region.offset.1..region.offset.1 + region.size.1 {
                for x in region.offset.0..region.offset.0 + region.size.0 {
                    let uv = Vec2::new(
                        (x as f32 + 0.5) / resolution.0 as f32,
                        (y as f32 + 0.5) / resolution.1 as f32,
                    );
                    let base = original_pixel(&original, &images, uv).unwrap();
                    let paint = targets
                        .get(entity)
                        .unwrap()
                        .surface()
                        .surface()
                        .get_pixel(x, y)
                        .unwrap();
                    let pixel = over(paint, base);
                    data.extend([
                        linear_to_srgb(pixel[0]),
                        linear_to_srgb(pixel[1]),
                        linear_to_srgb(pixel[2]),
                        (pixel[3].clamp(0.0, 1.0) * 255.0).round() as u8,
                    ]);
                }
            }
            if let Some(image) = images.get_mut(&texture)
                && let Some(bytes) = &mut image.data
            {
                let row_bytes = (region.size.0 * 4) as usize;
                for row in 0..region.size.1 as usize {
                    let offset = (((region.offset.1 as usize + row) * resolution.0 as usize)
                        + region.offset.0 as usize)
                        * 4;
                    bytes[offset..offset + row_bytes]
                        .copy_from_slice(&data[row * row_bytes..(row + 1) * row_bytes]);
                }
            }
        }
        let appearance = targets.appearances.get_mut(&entity).unwrap();
        if has_paint {
            let painted = appearance.painted_handle.get_or_insert_with(|| {
                let mut material = original.clone();
                material.base_color = Color::WHITE;
                material.base_color_texture = Some(texture.clone());
                materials.add(material)
            });
            // Replacing Image data can replace its GPU texture view. A material
            // added directly to Assets has to rebuild its texture bind group,
            // otherwise the renderer may keep sampling the previous GPU image.
            if let Some(material) = materials.get_mut(&*painted) {
                material.base_color_texture = Some(texture.clone());
            }
            handle.0 = painted.clone();
        } else {
            handle.0 = appearance.original_handle.clone();
        }
        appearance.published_bytes = images.get(&texture).and_then(|i| i.data.clone());
        component.dirty = false;
    }
}

/// Read-only evidence from actual UV storage, bound Image and receiver geometry.
/// Enabled only for native qualification; it never dispatches editor commands.
fn projection_native_inspection(
    targets: Res<ProjectionTargets>,
    images: Res<Assets<Image>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    painting: Res<PaintingResource>,
    scene: ProjectionScene,
    names: Query<&Name>,
    visibility: Query<&InheritedVisibility>,
    mut previous: Local<HashMap<Entity, String>>,
) {
    if std::env::var_os("PENTIMENTO_NATIVE_DIAGNOSTICS").is_none() {
        return;
    }
    let Some(canvas_entity) = scene.active.entity else {
        return;
    };
    let Ok((canvas, canvas_transform, _)) = scene.canvases.get(canvas_entity) else {
        return;
    };
    let Some(pipeline) = painting.get_pipeline(canvas.plane_id) else {
        return;
    };
    let Ok((camera_transform, Some(camera))) = scene.camera.single() else {
        return;
    };
    let pixels = pipeline.layers.composited_surface().surface().pixels();
    let mut min = UVec2::splat(u32::MAX);
    let mut max = UVec2::ZERO;
    for (i, pixel) in pixels.iter().enumerate().filter(|(_, p)| p[3] > 0.0) {
        let _ = pixel;
        let xy = UVec2::new(i as u32 % canvas.width, i as u32 / canvas.width);
        min = min.min(xy);
        max = max.max(xy + UVec2::ONE);
    }
    let source_bounds = (min.x != u32::MAX
        && visibility.get(canvas_entity).ok().is_none_or(|v| v.get()))
    .then(|| {
        diagnostic_bounds(
            camera,
            camera_transform,
            [
                min.as_vec2(),
                Vec2::new(max.x as f32, min.y as f32),
                max.as_vec2(),
                Vec2::new(min.x as f32, max.y as f32),
            ]
            .into_iter()
            .map(|pixel| {
                let uv = pixel / Vec2::new(canvas.width as f32, canvas.height as f32);
                canvas_transform.transform_point(Vec3::new(
                    (uv.x - 0.5) * canvas.world_width,
                    (0.5 - uv.y) * canvas.world_height,
                    0.,
                ))
            }),
        )
    })
    .flatten();
    for (entity, handle, transform, _, _, material_handle) in &scene.mesh_query {
        let Some(target) = targets.get(entity) else {
            continue;
        };
        let Some(texture) = targets.get_texture(entity) else {
            continue;
        };
        let Some(image) = images.get(texture).and_then(|image| image.data.as_ref()) else {
            continue;
        };
        let Some(mesh) = meshes.get(&handle.0) else {
            continue;
        };
        let atlas = target.surface().surface().pixels();
        let texels = atlas.iter().filter(|p| p[3] > 0.).count();
        let atlas_hash = diagnostic_hash(bytemuck::cast_slice(atlas));
        let image_hash = diagnostic_hash(image);
        let geometry = format!(
            "{:?}|{:?}|{:?}",
            mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            mesh.attribute(Mesh::ATTRIBUTE_UV_0),
            mesh.indices()
        );
        let geometry_hash = diagnostic_hash(geometry.as_bytes());
        let bound = materials
            .get(&material_handle.0)
            .is_some_and(|m| m.base_color_texture.as_ref() == Some(texture));
        let bounds = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(positions)) => diagnostic_bounds(
                camera,
                camera_transform,
                positions
                    .iter()
                    .map(|p| transform.transform_point(Vec3::from(*p))),
            ),
            _ => None,
        };
        let receipt = format!(
            "entity={} name={:?} texels={} atlas={:016x} image={:016x} geometry={:016x} bound={} undo={} redo={} active={} source_bounds={:?} target_bounds={:?}",
            entity.to_bits(),
            names.get(entity).map(Name::as_str).unwrap_or("unnamed"),
            texels,
            atlas_hash,
            image_hash,
            geometry_hash,
            bound,
            pipeline.undo_count(),
            pipeline.redo_count(),
            pipeline.is_stroking(),
            source_bounds,
            bounds
        );
        if previous.get(&entity) != Some(&receipt) {
            info!("Projection target receipt: {}", receipt);
            previous.insert(entity, receipt);
        }
    }
}

fn diagnostic_hash(bytes: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut hash = std::hash::DefaultHasher::new();
    hash.write(bytes);
    hash.finish()
}

fn diagnostic_bounds(
    camera: &Camera,
    transform: &GlobalTransform,
    positions: impl IntoIterator<Item = Vec3>,
) -> Option<[f32; 4]> {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for point in positions {
        if let Ok(point) = camera.world_to_viewport(transform, point) {
            min = min.min(point);
            max = max.max(point);
        }
    }
    min.is_finite().then_some([min.x, min.y, max.x, max.y])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn context() -> ProjectionContext {
        ProjectionContext {
            camera: Vec3::new(0.0, 0.0, 2.0),
            invert_culling: false,
            world_to_canvas: Mat4::from_translation(-Vec3::Z),
            size: Vec2::splat(2.0),
            resolution: UVec2::new(2, 2),
        }
    }

    #[test]
    fn canvas_pixel_orientation_and_nonuniform_transform() {
        let mut context = context();
        assert_eq!(source_pixel(Vec3::new(-1.0, 1.0, 0.0), &context), Some(0));
        assert_eq!(source_pixel(Vec3::new(1.0, 1.0, 0.0), &context), Some(1));
        assert_eq!(source_pixel(Vec3::new(-1.0, -1.0, 0.0), &context), Some(2));
        assert_eq!(source_pixel(Vec3::new(1.0, -1.0, 0.0), &context), Some(3));
        assert!(source_pixel(Vec3::new(0.0, 0.0, 1.5), &context).is_none());
        context.world_to_canvas = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 3.0, 1.0),
            Quat::IDENTITY,
            Vec3::Z,
        )
        .inverse();
        assert_eq!(source_pixel(Vec3::new(2.0, -3.0, 0.0), &context), Some(3));
    }

    #[test]
    fn world_normal_uses_inverse_transpose() {
        let normal = Vec3::new(1.0, 1.0, 1.0).normalize();
        let mesh = MeshRaycastData {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            indices: vec![0, 1, 2],
            normals: vec![normal; 3],
            uvs: vec![Vec2::ZERO, Vec2::X, Vec2::Y],
            tangents: Vec::new(),
        };
        let transform = GlobalTransform::from(Transform::from_scale(Vec3::new(2.0, 3.0, 4.0)));
        let triangle = ProjectionTriangle::from_mesh(
            Entity::from_bits(1),
            &mesh,
            0,
            &transform,
            Some(Face::Back),
        )
        .unwrap();
        let expected = Vec3::new(0.5, 1.0 / 3.0, 0.25).normalize();
        assert!((triangle.normal(Vec3::new(0.2, 0.3, 0.5)) - expected).length() < 1e-6);
    }

    #[test]
    fn atlas_rasterization_has_no_holes_and_obeys_occluders() {
        let target = Entity::from_bits(1);
        let blocker = Entity::from_bits(2);
        let mesh = Rectangle::new(4.0, 4.0).mesh().build();
        let data = extract_mesh_raycast_data(&mesh).unwrap();
        let transform = GlobalTransform::IDENTITY;
        let mut triangles: Vec<_> = (0..data.triangle_count())
            .map(|face| {
                ProjectionTriangle::from_mesh(target, &data, face, &transform, Some(Face::Back))
                    .unwrap()
            })
            .collect();
        let resolutions = HashMap::from([(target, (8, 8))]);
        let mapping = build_mapping(&context(), &triangles, &resolutions);
        assert_eq!(mapping[&target].iter().filter(|p| p.is_some()).count(), 64);
        assert_eq!(mapping[&target][0], Some(0));
        assert_eq!(mapping[&target][7], Some(1));
        assert_eq!(mapping[&target][56], Some(2));
        assert_eq!(mapping[&target][63], Some(3));
        // A nearer, UV-less mesh still blocks projection onto the rear mesh.
        let blocking_transform = GlobalTransform::from_translation(Vec3::new(0.0, 0.0, 0.5));
        for face in 0..data.triangle_count() {
            let mut triangle = ProjectionTriangle::from_mesh(
                blocker,
                &data,
                face,
                &blocking_transform,
                Some(Face::Back),
            )
            .unwrap();
            triangle.uvs = None;
            triangles.push(triangle);
        }
        let mapping = build_mapping(&context(), &triangles, &resolutions);
        assert!(mapping[&target].iter().all(Option::is_none));
    }

    #[test]
    fn material_facing_culls_receivers_and_occluders_consistently() {
        let target = Entity::from_bits(1);
        let blocker = Entity::from_bits(2);
        let mut data = extract_mesh_raycast_data(&Rectangle::new(4.0, 4.0).mesh().build()).unwrap();
        // Shading normals deliberately disagree with winding: raster culling
        // depends only on geometry and material, not a smoothed normal field.
        data.normals.fill(Vec3::NEG_Z);
        let resolutions = HashMap::from([(target, (8, 8))]);
        for mirrored in [false, true] {
            for invert_culling in [false, true] {
                let mut context = context();
                context.invert_culling = invert_culling;
                let transform = GlobalTransform::from(Transform::from_scale(Vec3::new(
                    if mirrored { -1.0 } else { 1.0 },
                    1.0,
                    1.0,
                )));
                for cull_mode in [Some(Face::Back), Some(Face::Front), None] {
                    let triangles: Vec<_> = (0..data.triangle_count())
                        .map(|face| {
                            ProjectionTriangle::from_mesh(
                                target, &data, face, &transform, cull_mode,
                            )
                            .unwrap()
                        })
                        .collect();
                    let mapping = build_mapping(&context, &triangles, &resolutions);
                    let front = !mirrored != invert_culling;
                    let expected = match cull_mode {
                        Some(Face::Back) => front,
                        Some(Face::Front) => !front,
                        None => true,
                    };
                    assert_eq!(
                        mapping[&target].iter().filter(|p| p.is_some()).count(),
                        if expected { 64 } else { 0 },
                        "mirrored={mirrored}, invert={invert_culling}, cull={cull_mode:?}"
                    );
                }
            }
        }
        for cull_mode in [Some(Face::Back), Some(Face::Front), None] {
            let mut triangles: Vec<_> = (0..data.triangle_count())
                .map(|face| {
                    ProjectionTriangle::from_mesh(
                        target,
                        &data,
                        face,
                        &GlobalTransform::IDENTITY,
                        Some(Face::Back),
                    )
                    .unwrap()
                })
                .collect();
            let transform = GlobalTransform::from(
                Transform::from_xyz(0.0, 0.0, 0.5).with_scale(Vec3::new(-1.0, 1.0, 1.0)),
            );
            for face in 0..data.triangle_count() {
                let mut triangle =
                    ProjectionTriangle::from_mesh(blocker, &data, face, &transform, cull_mode)
                        .unwrap();
                triangle.uvs = None;
                triangles.push(triangle);
            }
            let mapping = build_mapping(&context(), &triangles, &resolutions);
            assert_eq!(
                mapping[&target].iter().filter(|p| p.is_some()).count(),
                if cull_mode == Some(Face::Back) { 64 } else { 0 }
            );
        }
    }

    #[test]
    fn material_cull_mode_reaches_live_mapping_and_changes_invalidate_occlusion() {
        for cull_mode in [Some(Face::Back), Some(Face::Front), None] {
            let (mut app, target, original) = test_app();
            app.world_mut()
                .get_mut::<Transform>(target)
                .unwrap()
                .scale
                .x = -1.0;
            app.world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .get_mut(&original)
                .unwrap()
                .cull_mode = cull_mode;
            set_canvas(&mut app, &[[1.0, 0.0, 0.0, 1.0]; 4]);
            app.update();
            let pixels = output(&app, target);
            assert!(pixels.chunks_exact(4).all(|pixel| pixel
                == if cull_mode == Some(Face::Back) {
                    [255, 255, 255, 255]
                } else {
                    [255, 0, 0, 255]
                }));
        }
        let (mut app, target, _) = test_app();
        let mut mesh = Rectangle::new(4.0, 4.0).mesh().build();
        mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0);
        let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        app.world_mut().spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(0.0, 0.0, 0.5).with_scale(Vec3::new(-1.0, 1.0, 1.0)),
            InheritedVisibility::VISIBLE,
        ));
        set_canvas(&mut app, &[[1.0, 0.0, 0.0, 1.0]; 4]);
        app.update();
        assert!(
            output(&app, target)
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
        // double_sided changes lighting only; cull_mode=None changes visibility.
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&material)
            .unwrap()
            .cull_mode = None;
        app.update();
        assert!(
            output(&app, target)
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 255, 255, 255])
        );
    }

    pub(crate) fn test_app() -> (App, Entity, Handle<StandardMaterial>) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin));
        app.init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>();
        app.init_resource::<PaintingResource>()
            .init_resource::<ActiveCanvasPlane>()
            .insert_resource(ProjectionMode {
                enabled: true,
                live_projection: true,
            })
            .add_message::<ProjectionEvent>()
            .add_plugins(ProjectionPaintingPlugin);
        app.world_mut()
            .spawn((MainCamera, Transform::from_xyz(0.0, 0.0, 2.0)));
        let canvas = app
            .world_mut()
            .spawn((
                CanvasPlane::new(0, 2, 2, 2.0, 2.0),
                Transform::from_xyz(0.0, 0.0, 1.0),
            ))
            .id();
        app.world_mut().resource_mut::<ActiveCanvasPlane>().entity = Some(canvas);
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::new(4.0, 4.0));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::WHITE,
                ..default()
            });
        let target = app
            .world_mut()
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                InheritedVisibility::VISIBLE,
                ProjectionTarget::uv_atlas((8, 8)),
            ))
            .id();
        app.world_mut()
            .resource_mut::<PaintingResource>()
            .get_or_create_pipeline(0, 2, 2);
        (app, target, material)
    }

    pub(crate) fn set_canvas(app: &mut App, colors: &[[f32; 4]]) {
        let mut painting = app.world_mut().resource_mut::<PaintingResource>();
        let pipeline = painting.get_pipeline_mut(0).unwrap();
        pipeline
            .layers
            .active_layer_mut()
            .unwrap()
            .surface
            .surface_mut()
            .pixels_mut()
            .copy_from_slice(colors);
    }

    pub(crate) fn output(app: &App, target: Entity) -> Vec<u8> {
        let targets = app.world().resource::<ProjectionTargets>();
        app.world()
            .resource::<Assets<Image>>()
            .get(targets.get_texture(target).unwrap())
            .unwrap()
            .data
            .clone()
            .unwrap()
    }

    #[test]
    fn live_projection_updates_material_image_and_is_idempotent() {
        let (mut app, target, original) = test_app();
        let colors = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0; 4],
        ];
        set_canvas(&mut app, &colors);
        app.update();
        let pixels = output(&app, target);
        assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[28..32], &[0, 255, 0, 255]);
        assert_eq!(&pixels[224..228], &[0, 0, 255, 255]);
        assert_eq!(&pixels[252..256], &[255, 255, 255, 255]); // Original white, not transparent black.
        let material = &app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(target)
            .unwrap()
            .0;
        assert_ne!(material, &original);
        assert!(
            app.world()
                .resource::<Assets<StandardMaterial>>()
                .get(&original)
                .unwrap()
                .base_color_texture
                .is_none()
        );
        app.update();
        assert_eq!(output(&app, target), pixels);
        assert!(
            !app.world()
                .resource::<ProjectionTargets>()
                .get(target)
                .unwrap()
                .has_dirty_regions()
        );
        // Erase and undo-to-empty replace the layer, restoring original material.
        set_canvas(&mut app, &[[0.0; 4]; 4]);
        app.update();
        assert_eq!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
        assert!(
            output(&app, target)
                .chunks_exact(4)
                .all(|p| p == [255, 255, 255, 255])
        );
    }

    #[test]
    fn explicit_project_and_clear_work_without_live_mode() {
        let (mut app, target, original) = test_app();
        app.world_mut()
            .resource_mut::<ProjectionMode>()
            .live_projection = false;
        set_canvas(&mut app, &[[1.0, 0.0, 0.0, 0.5]; 4]);
        app.world_mut()
            .write_message(ProjectionEvent::ProjectToScene);
        app.update();
        let once = output(&app, target);
        app.world_mut()
            .write_message(ProjectionEvent::ProjectToScene);
        app.update();
        assert_eq!(output(&app, target), once);
        app.world_mut()
            .write_message(ProjectionEvent::ClearProjection {
                mesh_entity: target,
            });
        app.update();
        assert_eq!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
    }

    #[test]
    fn source_stroke_undo_updates_live_projection() {
        let (mut app, target, original) = test_app();
        {
            let mut resource = app.world_mut().resource_mut::<PaintingResource>();
            let pipeline = resource.get_pipeline_mut(0).unwrap();
            pipeline.set_color([1.0, 0.0, 0.0, 1.0]);
            pipeline.begin_stroke(0, 1, 0);
            pipeline.stroke_to(0.5, 0.5, 1.0);
            pipeline.end_stroke();
        }
        app.update();
        assert_ne!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
        assert!(app.world_mut().resource_mut::<PaintingResource>().undo(0));
        app.update();
        assert_eq!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
    }
    #[test]
    fn layered_source_redo_and_cancel_restore_actual_target_image_and_preserve_geometry() {
        let (mut app, target, original_material) = test_app();
        app.update();
        let mesh_handle = app.world().get::<Mesh3d>(target).unwrap().0.clone();
        let before_mesh = format!(
            "{:?}",
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&mesh_handle)
                .unwrap()
        );
        let pristine = output(&app, target);
        for id in 1..=2 {
            let mut painting = app.world_mut().resource_mut::<PaintingResource>();
            let p = painting.get_pipeline_mut(0).unwrap();
            p.set_color([1., 0., 1., 0.4]);
            p.begin_stroke(0, id, 0);
            p.stroke_to(0.5, 0.5, 1.);
            p.end_stroke();
            drop(painting);
            app.update();
        }
        let painted = output(&app, target);
        assert_ne!(painted, pristine);
        for _ in 0..3 {
            assert!(app.world_mut().resource_mut::<PaintingResource>().undo(0));
            app.update();
            let first = output(&app, target);
            assert_ne!(first, painted);
            {
                let mut painting = app.world_mut().resource_mut::<PaintingResource>();
                let p = painting.get_pipeline_mut(0).unwrap();
                p.set_color([0., 1., 0., 1.]);
                p.begin_stroke(0, 99, 0);
                p.stroke_to(1.5, 1.5, 1.);
            }
            app.update();
            assert_ne!(output(&app, target), first);
            app.world_mut()
                .resource_mut::<PaintingResource>()
                .get_pipeline_mut(0)
                .unwrap()
                .cancel_stroke();
            app.update();
            assert_eq!(output(&app, target), first);
            assert!(app.world_mut().resource_mut::<PaintingResource>().redo(0));
            app.update();
            assert_eq!(output(&app, target), painted);
        }
        assert!(app.world_mut().resource_mut::<PaintingResource>().undo(0));
        assert!(app.world_mut().resource_mut::<PaintingResource>().undo(0));
        app.update();
        assert_eq!(output(&app, target), pristine);
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            original_material
        );
        assert_eq!(
            format!(
                "{:?}",
                app.world()
                    .resource::<Assets<Mesh>>()
                    .get(&mesh_handle)
                    .unwrap()
            ),
            before_mesh
        );
        assert!(app.world_mut().resource_mut::<PaintingResource>().redo(0));
        assert!(app.world_mut().resource_mut::<PaintingResource>().redo(0));
        app.update();
        assert_eq!(output(&app, target), painted);
    }

    #[test]
    fn modified_mesh_invalidates_live_coverage() {
        let (mut app, target, original) = test_app();
        set_canvas(&mut app, &[[1.0, 0.0, 0.0, 1.0]; 4]);
        app.update();
        let mesh = app.world().get::<Mesh3d>(target).unwrap().0.clone();
        {
            let mut assets = app.world_mut().resource_mut::<Assets<Mesh>>();
            let mesh = assets.get_mut(&mesh).unwrap();
            let VertexAttributeValues::Float32x3(positions) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!("positions");
            };
            for position in positions {
                position[0] += 100.0;
            }
        }
        app.update();
        assert_eq!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
    }

    #[test]
    fn ptex_target_is_not_silently_treated_as_a_uv_atlas() {
        let (mut app, target, original) = test_app();
        app.world_mut()
            .entity_mut(target)
            .insert(ProjectionTarget::ptex(32));
        set_canvas(&mut app, &[[1.0, 0.0, 0.0, 1.0]; 4]);
        app.update();
        assert!(
            app.world()
                .resource::<ProjectionTargets>()
                .get(target)
                .is_none()
        );
        assert_eq!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
    }

    #[test]
    fn distinct_canvases_keep_independent_projection_layers() {
        let (mut app, target, _) = test_app();
        set_canvas(&mut app, &[[1.0, 0.0, 0.0, 1.0]; 4]);
        app.update();
        let canvas = app
            .world_mut()
            .spawn((
                CanvasPlane::new(1, 2, 2, 2.0, 2.0),
                Transform::from_xyz(0.0, 0.0, 1.0),
            ))
            .id();
        app.world_mut().resource_mut::<ActiveCanvasPlane>().entity = Some(canvas);
        {
            let mut painting = app.world_mut().resource_mut::<PaintingResource>();
            let pipeline = painting.get_or_create_pipeline(1, 2, 2);
            pipeline.clear([0.0, 0.0, 1.0, 1.0]);
        }
        app.update();
        assert!(
            output(&app, target)
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
        app.world_mut()
            .resource_mut::<PaintingResource>()
            .get_pipeline_mut(1)
            .unwrap()
            .clear([0.0; 4]);
        app.update();
        assert!(
            output(&app, target)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
    }

    #[test]
    fn base_texture_resolution_and_color_are_preserved() {
        let (mut app, target, original) = test_app();
        let image = Image::new_fill(
            Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 255, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        );
        let texture = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&original)
            .unwrap()
            .base_color_texture = Some(texture.clone());
        set_canvas(
            &mut app,
            &[[1.0, 0.0, 0.0, 1.0], [0.0; 4], [0.0; 4], [0.0; 4]],
        );
        app.update();
        assert_eq!(&output(&app, target)[28..32], &[0, 255, 0, 255]);
        assert_eq!(
            app.world()
                .resource::<Assets<StandardMaterial>>()
                .get(&original)
                .unwrap()
                .base_color_texture,
            Some(texture)
        );
    }
    #[test]
    fn same_frame_clear_and_toggle_cancellation_respect_command_order() {
        let (mut app, target, original) = test_app();
        app.world_mut()
            .resource_mut::<ProjectionMode>()
            .live_projection = false;
        set_canvas(&mut app, &[[1.0, 0.0, 0.0, 1.0]; 4]);
        app.world_mut()
            .write_message(ProjectionEvent::SetLiveProjection { enabled: true });
        app.world_mut()
            .write_message(ProjectionEvent::SetLiveProjection { enabled: false });
        app.update();
        assert_eq!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
        app.world_mut()
            .write_message(ProjectionEvent::ProjectToScene);
        app.world_mut()
            .write_message(ProjectionEvent::ClearAllProjections);
        app.update();
        assert_eq!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
        app.world_mut()
            .write_message(ProjectionEvent::ClearAllProjections);
        app.world_mut()
            .write_message(ProjectionEvent::ProjectToScene);
        app.update();
        assert_ne!(
            &app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(target)
                .unwrap()
                .0,
            &original
        );
    }
    #[test]
    fn image_updates_invalidate_the_material_gpu_binding() {
        let (mut app, target, _) = test_app();
        set_canvas(&mut app, &[[1.0, 0.0, 0.0, 1.0]; 4]);
        app.update();
        app.update();
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(target)
            .unwrap()
            .0
            .id();
        let mut cursor = app
            .world()
            .resource::<Messages<AssetEvent<StandardMaterial>>>()
            .get_cursor_current();
        set_canvas(&mut app, &[[0.0, 0.0, 1.0, 1.0]; 4]);
        app.update();
        app.update();
        assert!(
            cursor
                .read(
                    app.world()
                        .resource::<Messages<AssetEvent<StandardMaterial>>>()
                )
                .any(|event| matches!(event, AssetEvent::Modified { id } if *id == material)),
            "The material must rebuild its binding after the GPU image view is replaced"
        );
    }
}
