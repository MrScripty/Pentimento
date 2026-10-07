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
struct ProjectionSession {
    context: ProjectionContext,
    geometry: Vec<MeshStamp>,
    revision: u64,
    mapping: HashMap<Entity, Vec<Option<usize>>>,
    source: Vec<[f32; 4]>,
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
        app.init_resource::<ProjectionTargets>()
            .init_resource::<MeshRaycastCache>()
            .add_systems(
                PostUpdate,
                (
                    register_projection_targets,
                    setup_projection_textures,
                    invalidate_mesh_cache,
                    live_projection_system,
                    upload_projection_textures,
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
    mut query: Query<(
        Entity,
        &mut ProjectionTarget,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (entity, mut component, material) in &mut query {
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
    let mut explicitly_requested = false;
    let mut enabling_requested = false;
    let mut trailing_clears = Vec::new();
    for event in events.read() {
        match event {
            ProjectionEvent::ProjectToScene => {
                explicitly_requested = true;
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
    let requested = explicitly_requested || enabling_requested;
    if !requested && !projection_mode.live_projection {
        return;
    }
    let Some(entity) = active.entity else {
        return;
    };
    let Ok((plane, transform, canvas_texture)) = canvases.get(entity) else {
        return;
    };
    let Ok((camera, view)) = camera.single() else {
        return;
    };
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
                context,
                geometry,
                revision: cache.revision,
                mapping,
                source: Vec::new(),
            },
        );
    }
    let session = targets.sessions.get_mut(&plane.plane_id).unwrap();
    session.source = pixels.to_vec();
    let layer = session
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
        if !has_paint {
            handle.0 = appearance.original_handle.clone();
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
        component.dirty = false;
    }
}

#[cfg(test)]
mod tests {
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

    fn test_app() -> (App, Entity, Handle<StandardMaterial>) {
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

    fn set_canvas(app: &mut App, colors: &[[f32; 4]]) {
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

    fn output(app: &App, target: Entity) -> Vec<u8> {
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
