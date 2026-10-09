//! Fail-closed surface validation for sculpt transactions.
//!
//! Connectivity and geometric embedding are separate checks. Global vertex IDs
//! join chunk copies; UVs never decide adjacency. A BVH tests *all* potentially
//! contacting triangles, including adjacent triangles away from their shared
//! simplex. Predicate arithmetic is f64 over the actual f32 stored positions.
//! Near contacts are conservatively rejected at a local, scale-relative tolerance.

use crate::chunking::{ChunkId, ChunkedMesh, MeshChunk};
use glam::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SafetyError {
    #[error("invalid sculpt topology: {0}")]
    Topology(String),
    #[error("sculpt surface contains a non-finite or degenerate face")]
    DegenerateFace,
    #[error("sculpt deformation reverses or flattens a face")]
    InvertedFace,
    #[error("sculpt surfaces intersect or overlap outside their shared boundary")]
    Intersection,
    #[error("sculpt safety check exceeded its bounded collision workload")]
    WorkLimit,
}

#[derive(Clone, Debug, PartialEq)]
struct Triangle {
    ids: [u32; 3],
    p: [DVec3; 3],
    chunk: ChunkId,
    min: DVec3,
    max: DVec3,
    tolerance: f64,
}

impl Triangle {
    fn new(ids: [u32; 3], p: [DVec3; 3], chunk: ChunkId) -> Result<Self, SafetyError> {
        let length = (p[1] - p[0])
            .length()
            .max((p[2] - p[1]).length())
            .max((p[0] - p[2]).length());
        if !p.iter().all(|p| p.is_finite())
            || length == 0.
            || (p[1] - p[0]).cross(p[2] - p[0]).length() <= length * length * 1e-10
        {
            return Err(SafetyError::DegenerateFace);
        }
        let tolerance =
            length * 1e-7 + p.iter().map(|p| p.abs().max_element()).fold(0., f64::max) * 1e-14;
        Ok(Self {
            ids,
            p,
            chunk,
            min: p[0].min(p[1]).min(p[2]) - DVec3::splat(tolerance),
            max: p[0].max(p[1]).max(p[2]) + DVec3::splat(tolerance),
            tolerance,
        })
    }
    fn normal(&self) -> DVec3 {
        (self.p[1] - self.p[0]).cross(self.p[2] - self.p[0])
    }
    fn key(&self) -> [u32; 3] {
        let mut ids = self.ids;
        ids.sort_unstable();
        ids
    }
}

type VertexIdentityMap = HashMap<painting::half_edge::VertexId, painting::half_edge::VertexId>;

#[derive(Clone, Debug)]
struct ChunkGeometryWitness {
    mesh: painting::half_edge::HalfEdgeMesh,
    local_to_original: VertexIdentityMap,
    original_to_local: VertexIdentityMap,
}

/// Exact snapshot used only to reuse an already proved admission. Equality
/// includes positions, connectivity, UVs/normals, the private half-edge map and
/// both global identity maps. Dirty/render flags do not change surface validity.
/// A mismatch triggers full reading and incremental geometric admission.
#[derive(Clone, Debug)]
pub(crate) struct GeometryWitness {
    chunks: HashMap<ChunkId, ChunkGeometryWitness>,
}
impl GeometryWitness {
    pub(crate) fn capture(mesh: &ChunkedMesh) -> Self {
        Self {
            chunks: mesh
                .chunks
                .iter()
                .map(|(&id, c)| {
                    (
                        id,
                        ChunkGeometryWitness {
                            mesh: c.mesh.clone(),
                            local_to_original: c.local_to_original.clone(),
                            original_to_local: c.original_to_local.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
    pub(crate) fn matches(&self, mesh: &ChunkedMesh) -> bool {
        self.chunks.len() == mesh.chunks.len()
            && mesh.chunks.iter().all(|(&id, c)| {
                id == c.id
                    && self.chunks.get(&id).is_some_and(|snapshot| {
                        snapshot.mesh == c.mesh
                            && snapshot.local_to_original == c.local_to_original
                            && snapshot.original_to_local == c.original_to_local
                    })
            })
    }
}

/// A geometry-only snapshot. It is cheap to compare against a candidate without
/// duplicating half-edge storage on every dab.
#[derive(Debug, Clone)]
pub(crate) struct Surface {
    triangles: Vec<Triangle>,
}

impl Surface {
    pub(crate) fn read(mesh: &ChunkedMesh) -> Result<Self, SafetyError> {
        let mut triangles = Vec::new();
        if mesh.chunks.iter().any(|(&id, chunk)| id != chunk.id) {
            return Err(SafetyError::Topology(
                "chunk map key/identity mismatch".into(),
            ));
        }
        let mut chunks: Vec<_> = mesh.chunks.values().collect();
        chunks.sort_by_key(|chunk| chunk.id.0);
        for chunk in chunks {
            Self::append_chunk(chunk, &mut triangles)?;
        }
        Ok(Self { triangles })
    }

    fn append_chunk(chunk: &MeshChunk, triangles: &mut Vec<Triangle>) -> Result<(), SafetyError> {
        chunk
            .mesh
            .validate()
            .map_err(|e| SafetyError::Topology(e.to_string()))?;
        for face in chunk.mesh.faces() {
            let vertices = chunk.mesh.get_face_vertices(face.id);
            if vertices.is_empty() {
                continue;
            } // Explicit collapse tombstones only; validate() checked them.
            if vertices.len() != 3 {
                return Err(SafetyError::Topology("non-triangular face".into()));
            }
            let mut ids = [0; 3];
            let mut p = [DVec3::ZERO; 3];
            for i in 0..3 {
                ids[i] = chunk
                    .local_to_original
                    .get(&vertices[i])
                    .ok_or_else(|| SafetyError::Topology("missing global vertex identity".into()))?
                    .0;
                if chunk
                    .original_to_local
                    .get(&painting::half_edge::VertexId(ids[i]))
                    != Some(&vertices[i])
                {
                    return Err(SafetyError::Topology(
                        "inconsistent global vertex identity".into(),
                    ));
                }
                p[i] = chunk.mesh.vertex(vertices[i]).unwrap().position.as_dvec3();
            }
            triangles.push(Triangle::new(ids, p, chunk.id)?);
        }
        Ok(())
    }

    /// Internal adaptive edits remain unobservable until the whole dab commits.
    /// Check local connectivity, finite/valid facets and orientation per edit;
    /// the pipeline checks the complete global candidate before returning.
    pub(crate) fn read_chunk(chunk: &MeshChunk) -> Result<Self, SafetyError> {
        let mut triangles = Vec::new();
        Self::append_chunk(chunk, &mut triangles)?;
        Ok(Self { triangles })
    }

    pub(crate) fn chunk(&self, id: ChunkId) -> Self {
        Self {
            triangles: self
                .triangles
                .iter()
                .filter(|t| t.chunk == id)
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn replace_chunk_geometry(&mut self, id: ChunkId, replacement: Self) {
        debug_assert!(replacement.triangles.iter().all(|t| t.chunk == id));
        self.triangles.retain(|t| t.chunk != id);
        self.triangles.extend(replacement.triangles);
    }

    /// Refresh positions after a deformation-only stage. Topology cannot change
    /// in that stage; retain its admitted incidence rather than walking every
    /// half-edge again. Global IDs still resolve any synchronized chunk copies.
    pub(crate) fn deformed(&self, mesh: &ChunkedMesh) -> Result<Self, SafetyError> {
        let mut triangles = Vec::with_capacity(self.triangles.len());
        for t in &self.triangles {
            let chunk = mesh
                .get_chunk(t.chunk)
                .ok_or_else(|| SafetyError::Topology("missing chunk during deformation".into()))?;
            let mut p = [DVec3::ZERO; 3];
            for (i, position) in p.iter_mut().enumerate() {
                let local = chunk
                    .original_to_local
                    .get(&painting::half_edge::VertexId(t.ids[i]))
                    .ok_or_else(|| {
                        SafetyError::Topology("missing vertex during deformation".into())
                    })?;
                *position = chunk
                    .mesh
                    .vertex(*local)
                    .ok_or_else(|| {
                        SafetyError::Topology("invalid vertex during deformation".into())
                    })?
                    .position
                    .as_dvec3();
            }
            triangles.push(Triangle::new(t.ids, p, t.chunk)?);
        }
        Ok(Self { triangles })
    }

    /// Previously admitted, unchanged triangle pairs cannot become intersecting.
    /// Test changed facets against the entire candidate BVH, including distant
    /// unmodified chunks, and recheck incidence when a topology edit changed it.
    pub(crate) fn validate_change(&self, before: &Self) -> Result<(), SafetyError> {
        if self.triangles == before.triangles {
            return Ok(());
        }
        let old: HashMap<_, _> = before.triangles.iter().map(|t| (t.key(), t)).collect();
        let changed: Vec<_> = self
            .triangles
            .iter()
            .map(|t| {
                old.get(&t.key()).is_none_or(|previous| {
                    previous.ids != t.ids || previous.p != t.p || previous.chunk != t.chunk
                })
            })
            .collect();
        let topology_changed = self.triangles.len() != before.triangles.len()
            || self.triangles.iter().any(|t| {
                old.get(&t.key())
                    .is_none_or(|previous| previous.ids != t.ids || previous.chunk != t.chunk)
            });
        if !changed.iter().any(|&value| value) && !topology_changed {
            return Ok(());
        }
        let mut affected: HashSet<_> = self
            .triangles
            .iter()
            .zip(&changed)
            .filter(|(_, changed)| **changed)
            .flat_map(|(t, _)| t.ids)
            .collect();
        if topology_changed {
            let keys: HashSet<_> = self.triangles.iter().map(Triangle::key).collect();
            for t in &before.triangles {
                if !keys.contains(&t.key()) {
                    affected.extend(t.ids);
                }
            }
            // Only links incident to changed/removed facets can change. Gather
            // their complete global incidence, including unchanged neighbors.
            self.validate_incidence_for(Some(&affected))?;
        } else {
            let mut positions = HashMap::new();
            for t in &self.triangles {
                for i in 0..3 {
                    if !affected.contains(&t.ids[i]) {
                        continue;
                    }
                    if positions
                        .insert(t.ids[i], t.p[i])
                        .is_some_and(|old| old != t.p[i])
                    {
                        return Err(SafetyError::Topology(
                            "chunk boundary position mismatch".into(),
                        ));
                    }
                }
            }
        }
        if !changed.iter().any(|&value| value) {
            return Ok(());
        }
        let mut indices: Vec<_> = (0..self.triangles.len()).collect();
        let bvh = Bvh::build(&self.triangles, &mut indices);
        let mut remaining = 2_000_000usize;
        for (i, t) in self.triangles.iter().enumerate() {
            if changed[i] {
                bvh.check_changed(i, t, &self.triangles, &changed, &mut remaining)?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), SafetyError> {
        self.validate_incidence_for(None)?;
        if self.triangles.is_empty() {
            return Ok(());
        }
        let mut indices: Vec<_> = (0..self.triangles.len()).collect();
        let bvh = Bvh::build(&self.triangles, &mut indices);
        // Deterministic work cap, not a time-dependent early success. Pathological
        // dense candidates fail closed rather than freezing the interactive tool.
        let mut remaining = 2_000_000usize;
        let changed = vec![true; self.triangles.len()];
        for (i, triangle) in self.triangles.iter().enumerate() {
            bvh.check_changed(i, triangle, &self.triangles, &changed, &mut remaining)?;
        }
        Ok(())
    }

    fn validate_incidence_for(&self, affected: Option<&HashSet<u32>>) -> Result<(), SafetyError> {
        let invalid = |s: &str| SafetyError::Topology(s.into());
        let mut positions = HashMap::new();
        let mut directed = HashSet::new();
        let mut facets = HashSet::new();
        let mut links: HashMap<u32, HashMap<u32, Vec<u32>>> = HashMap::new();
        for t in &self.triangles {
            if affected.is_some_and(|vertices| t.ids.iter().all(|id| !vertices.contains(id))) {
                continue;
            }
            if !facets.insert(t.key()) {
                return Err(invalid("duplicate facet"));
            }
            for i in 0..3 {
                let (id, next, prev) = (t.ids[i], t.ids[(i + 1) % 3], t.ids[(i + 2) % 3]);
                if affected.is_some_and(|vertices| !vertices.contains(&id)) {
                    continue;
                }
                if positions
                    .insert(id, t.p[i])
                    .is_some_and(|old| old != t.p[i])
                {
                    return Err(invalid("chunk boundary position mismatch"));
                }
                if id == next || !directed.insert((id, next)) {
                    return Err(invalid("non-manifold edge incidence"));
                }
                let link = links.entry(id).or_default();
                link.entry(prev).or_default().push(next);
                link.entry(next).or_default().push(prev);
            }
        }
        for link in links.values() {
            let ends = link.values().filter(|n| n.len() == 1).count();
            if (ends != 0 && ends != 2) || link.values().any(|n| n.is_empty() || n.len() > 2) {
                return Err(invalid("non-manifold vertex link"));
            }
            let mut visited = HashSet::new();
            let mut stack = vec![*link.keys().next().unwrap()];
            while let Some(v) = stack.pop() {
                if visited.insert(v) {
                    stack.extend(link[&v].iter().copied());
                }
            }
            if visited.len() != link.len() {
                return Err(invalid("disconnected vertex fans"));
            }
        }
        Ok(())
    }

    pub(crate) fn validate_orientation(&self, before: &Self) -> Result<(), SafetyError> {
        if self.triangles == before.triangles {
            return Ok(());
        }
        let old: HashMap<_, _> = before.triangles.iter().map(|t| (t.key(), t)).collect();
        for t in &self.triangles {
            let Some(previous) = old.get(&t.key()) else {
                continue;
            };
            if previous.normal().dot(t.normal())
                <= previous.normal().length() * t.normal().length() * 1e-8
            {
                return Err(SafetyError::InvertedFace);
            }
        }
        Ok(())
    }

    /// Conservative continuous test for non-adjacent triangles during the
    /// linear vertex displacement of one deformation stage. Swept separating
    /// axes prove clearance; unresolved close approaches reject, never pass.
    pub(crate) fn validate_motion(&self, before: &Self) -> Result<(), SafetyError> {
        // deformed() preserves face ordering and identity. Verify that exact
        // contract and compare by index, avoiding global key maps twice per dab.
        if self.triangles == before.triangles {
            return Ok(());
        }
        if self.triangles.len() != before.triangles.len() {
            return Err(SafetyError::Topology("deformation changed topology".into()));
        }
        let mut starts = Vec::with_capacity(self.triangles.len());
        let mut swept = self.triangles.clone();
        for (t, previous) in self.triangles.iter().zip(&before.triangles) {
            if t.ids != previous.ids || t.chunk != previous.chunk {
                return Err(SafetyError::Topology("deformation changed topology".into()));
            }
            let p = previous.p;
            if p == t.p {
                starts.push(p);
                continue;
            }
            // The signed projected area is quadratic in time. Check its exact
            // minimum too, so a face cannot flip twice between accepted states.
            let u = p[1] - p[0];
            let v = p[2] - p[0];
            let du = (t.p[1] - t.p[0]) - u;
            let dv = (t.p[2] - t.p[0]) - v;
            let n = u.cross(v);
            let a = du.cross(dv).dot(n);
            let b = (du.cross(v) + u.cross(dv)).dot(n);
            let c = n.length_squared();
            let mut minimum = c.min(a + b + c);
            if a > 0. {
                let time = (-b / (2. * a)).clamp(0., 1.);
                minimum = minimum.min((a * time + b) * time + c);
            }
            if minimum <= c * 1e-8 {
                return Err(SafetyError::InvertedFace);
            }
            starts.push(p);
        }
        for (t, p) in swept.iter_mut().zip(&starts) {
            for point in p {
                t.min = t.min.min(*point - DVec3::splat(t.tolerance));
                t.max = t.max.max(*point + DVec3::splat(t.tolerance));
            }
        }
        if swept.is_empty() {
            return Ok(());
        }
        let mut indices: Vec<_> = (0..swept.len()).collect();
        let bvh = Bvh::build(&swept, &mut indices);
        let mut remaining = 200_000usize;
        for (i, t) in swept.iter().enumerate() {
            if t.p != starts[i] {
                bvh.motion_candidates(i, t, &swept, &starts, &mut remaining)?;
            }
        }
        Ok(())
    }
}

/// Full read-only check of the stored chunked surface. This does not repair or
/// discard invalid faces. Floating-point tolerances are conservative; this
/// standalone query validates a stored state, not a motion trajectory.
pub fn validate_sculpt_surface(mesh: &ChunkedMesh) -> Result<(), SafetyError> {
    Surface::read(mesh)?.validate()
}

#[derive(Debug)]
enum Bvh {
    Leaf {
        min: DVec3,
        max: DVec3,
        faces: Vec<usize>,
    },
    Branch {
        min: DVec3,
        max: DVec3,
        left: Box<Self>,
        right: Box<Self>,
    },
}
impl Bvh {
    fn build(triangles: &[Triangle], ids: &mut [usize]) -> Self {
        let min = ids
            .iter()
            .fold(DVec3::splat(f64::INFINITY), |p, &i| p.min(triangles[i].min));
        let max = ids.iter().fold(DVec3::splat(f64::NEG_INFINITY), |p, &i| {
            p.max(triangles[i].max)
        });
        if ids.len() <= 8 {
            return Self::Leaf {
                min,
                max,
                faces: ids.to_vec(),
            };
        }
        let extent = max - min;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        let middle = ids.len() / 2;
        ids.select_nth_unstable_by(middle, |&a, &b| {
            (triangles[a].min[axis] + triangles[a].max[axis])
                .total_cmp(&(triangles[b].min[axis] + triangles[b].max[axis]))
                .then(a.cmp(&b))
        });
        let (left, right) = ids.split_at_mut(middle);
        Self::Branch {
            min,
            max,
            left: Box::new(Self::build(triangles, left)),
            right: Box::new(Self::build(triangles, right)),
        }
    }
    fn check_changed(
        &self,
        i: usize,
        t: &Triangle,
        all: &[Triangle],
        changed: &[bool],
        remaining: &mut usize,
    ) -> Result<(), SafetyError> {
        let (min, max) = match self {
            Self::Leaf { min, max, .. } | Self::Branch { min, max, .. } => (*min, *max),
        };
        if !overlaps(t.min, t.max, min, max) {
            return Ok(());
        }
        match self {
            Self::Leaf { faces, .. } => {
                for &j in faces {
                    if j == i
                        || (j < i && changed[j])
                        || !overlaps(t.min, t.max, all[j].min, all[j].max)
                    {
                        continue;
                    }
                    if *remaining == 0 {
                        return Err(SafetyError::WorkLimit);
                    }
                    *remaining -= 1;
                    if intersects(t, &all[j]) {
                        return Err(SafetyError::Intersection);
                    }
                }
            }
            Self::Branch { left, right, .. } => {
                left.check_changed(i, t, all, changed, remaining)?;
                right.check_changed(i, t, all, changed, remaining)?;
            }
        }
        Ok(())
    }
}
impl Bvh {
    fn motion_candidates(
        &self,
        i: usize,
        a: &Triangle,
        all: &[Triangle],
        starts: &[[DVec3; 3]],
        remaining: &mut usize,
    ) -> Result<(), SafetyError> {
        let (min, max) = match self {
            Self::Leaf { min, max, .. } | Self::Branch { min, max, .. } => (*min, *max),
        };
        if !overlaps(a.min, a.max, min, max) {
            return Ok(());
        }
        match self {
            Self::Leaf { faces, .. } => {
                for &j in faces {
                    let b = &all[j];
                    // Only moving faces issue queries. An earlier stationary
                    // face has not checked this pair; earlier moving faces have.
                    if j == i || (j < i && b.p != starts[j]) {
                        continue;
                    }
                    if a.ids.iter().any(|id| b.ids.contains(id))
                        || !overlaps(a.min, a.max, b.min, b.max)
                    {
                        continue;
                    }
                    if a.p == starts[i] && b.p == starts[j] {
                        continue;
                    }
                    if !motion_clear(
                        starts[i],
                        a.p,
                        starts[j],
                        b.p,
                        a.tolerance.max(b.tolerance),
                        0,
                        remaining,
                    )? {
                        return Err(SafetyError::Intersection);
                    }
                }
            }
            Self::Branch { left, right, .. } => {
                left.motion_candidates(i, a, all, starts, remaining)?;
                right.motion_candidates(i, a, all, starts, remaining)?;
            }
        }
        Ok(())
    }
}
fn motion_clear(
    a0: [DVec3; 3],
    a1: [DVec3; 3],
    b0: [DVec3; 3],
    b1: [DVec3; 3],
    eps: f64,
    depth: u8,
    remaining: &mut usize,
) -> Result<bool, SafetyError> {
    if *remaining == 0 {
        return Err(SafetyError::WorkLimit);
    }
    *remaining -= 1;
    let separates = |axis: DVec3| {
        let length = axis.length();
        if length < 1e-30 {
            return false;
        }
        let axis = axis / length;
        let (amin, amax) = a0
            .iter()
            .chain(a1.iter())
            .map(|p| p.dot(axis))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p), hi.max(p))
            });
        let (bmin, bmax) = b0
            .iter()
            .chain(b1.iter())
            .map(|p| p.dot(axis))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p), hi.max(p))
            });
        amax < bmin - eps || bmax < amin - eps
    };
    for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
        if separates(axis) {
            return Ok(true);
        }
    }
    for a in [a0, a1, b0, b1] {
        let normal = (a[1] - a[0]).cross(a[2] - a[0]);
        if separates(normal) {
            return Ok(true);
        }
        for i in 0..3 {
            if separates(normal.cross(a[(i + 1) % 3] - a[i])) {
                return Ok(true);
            }
        }
    }
    for a in [a0, a1] {
        for b in [b0, b1] {
            for i in 0..3 {
                for j in 0..3 {
                    if separates((a[(i + 1) % 3] - a[i]).cross(b[(j + 1) % 3] - b[j])) {
                        return Ok(true);
                    }
                }
            }
        }
    }
    if depth == 12 {
        return Ok(false);
    }
    let am = std::array::from_fn(|i| a0[i].lerp(a1[i], 0.5));
    let bm = std::array::from_fn(|i| b0[i].lerp(b1[i], 0.5));
    Ok(motion_clear(a0, am, b0, bm, eps, depth + 1, remaining)?
        && motion_clear(am, a1, bm, b1, eps, depth + 1, remaining)?)
}

fn overlaps(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> bool {
    a.cmple(d).all() && c.cmple(b).all()
}

fn intersects(a: &Triangle, b: &Triangle) -> bool {
    let eps = a.tolerance.max(b.tolerance);
    let mut shared = Vec::new();
    for (i, id) in a.ids.iter().enumerate() {
        if b.ids.contains(id) {
            shared.push(a.p[i]);
        }
    }
    let na = a.normal().normalize();
    let nb = b.normal().normalize();
    let da = b.p.map(|p| (p - a.p[0]).dot(na));
    let db = a.p.map(|p| (p - b.p[0]).dot(nb));
    if separated(da, eps) || separated(db, eps) {
        return false;
    }
    if da.iter().all(|d| d.abs() <= eps) && db.iter().all(|d| d.abs() <= eps) {
        let n = na.abs();
        let axis = if n.x >= n.y && n.x >= n.z {
            0
        } else if n.y >= n.z {
            1
        } else {
            2
        };
        let project = |p: DVec3| match axis {
            0 => DVec2::new(p.y, p.z),
            1 => DVec2::new(p.x, p.z),
            _ => DVec2::new(p.x, p.y),
        };
        let ap = a.p.map(project);
        let bp = b.p.map(project);
        // Two adjacent coplanar facets may meet only on opposite sides of
        // their common edge. Never excuse a narrow folded overlap with a
        // distance-to-edge tolerance: positive-area slivers are still overlap.
        if shared.len() == 2 {
            let edge = shared[1] - shared[0];
            let av = a.p[a.ids.iter().position(|id| !b.ids.contains(id)).unwrap()];
            let bv = b.p[b.ids.iter().position(|id| !a.ids.contains(id)).unwrap()];
            return edge.cross(av - shared[0]).dot(edge.cross(bv - shared[0])) >= 0.;
        }
        if shared.len() == 3 {
            return true;
        }
        for i in 0..3 {
            if inside_2d(ap[i], bp, eps) && !b.ids.contains(&a.ids[i]) {
                return true;
            }
            if inside_2d(bp[i], ap, eps) && !a.ids.contains(&b.ids[i]) {
                return true;
            }
            for j in 0..3 {
                // Straight non-collinear segments with an actual shared
                // endpoint cannot cross again. Collinear overlap is detected
                // by the non-shared endpoint containment tests above.
                if [a.ids[i], a.ids[(i + 1) % 3]]
                    .iter()
                    .any(|id| [b.ids[j], b.ids[(j + 1) % 3]].contains(id))
                {
                    continue;
                }
                let r = ap[(i + 1) % 3] - ap[i];
                let s = bp[(j + 1) % 3] - bp[j];
                let denominator = r.perp_dot(s);
                if denominator.abs() <= eps * r.length().max(s.length()) {
                    continue;
                }
                let delta = bp[j] - ap[i];
                let t = delta.perp_dot(s) / denominator;
                let u = delta.perp_dot(r) / denominator;
                if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) {
                    return true;
                }
            }
        }
        return false;
    }
    for (t, other, distances) in [(a, b, db), (b, a, da)] {
        for i in 0..3 {
            let j = (i + 1) % 3;
            if distances[i].abs() <= eps
                && inside_triangle(t.p[i], other, eps)
                && !other.ids.contains(&t.ids[i])
            {
                return true;
            }
            // An incident edge meets the other face's plane at the shared
            // vertex only. Exempt the known topological endpoint, not a fuzzy
            // tube around it; any other intersecting edge is a real contact.
            if other.ids.contains(&t.ids[i]) || other.ids.contains(&t.ids[j]) {
                continue;
            }
            if (distances[i] > 0. && distances[j] < 0.) || (distances[i] < 0. && distances[j] > 0.)
            {
                let fraction = distances[i] / (distances[i] - distances[j]);
                let p = t.p[i].lerp(t.p[j], fraction);
                if inside_triangle(p, other, eps) {
                    return true;
                }
            }
        }
    }
    false
}
fn separated(d: [f64; 3], eps: f64) -> bool {
    d.iter().all(|x| *x > eps) || d.iter().all(|x| *x < -eps)
}
fn inside_triangle(p: DVec3, t: &Triangle, eps: f64) -> bool {
    let normal = t.normal().normalize();
    (0..3).all(|i| {
        let edge = t.p[(i + 1) % 3] - t.p[i];
        edge.cross(p - t.p[i]).dot(normal) >= -eps * edge.length()
    })
}
fn inside_2d(p: DVec2, t: [DVec2; 3], eps: f64) -> bool {
    let sign = (t[1] - t[0]).perp_dot(t[2] - t[0]).signum();
    (0..3).all(|i| {
        let edge = t[(i + 1) % 3] - t[i];
        edge.perp_dot(p - t[i]) * sign >= -eps * edge.length()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tri(ids: [u32; 3], p: [[f64; 3]; 3]) -> Triangle {
        Triangle::new(ids, p.map(DVec3::from), ChunkId(0)).unwrap()
    }
    #[test]
    fn geometric_contact_is_not_manifoldness() {
        let base = tri([0, 1, 2], [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]]);
        for p in [
            [[0.5, 0.5, -1.], [0.5, 0.5, 1.], [1., 0.5, 0.]],
            [[0.2, 0.2, 0.], [0.8, 0.2, 0.], [0.2, 0.8, 0.]],
            [[-0.5, 0.5, 0.], [1.5, 0.5, 0.], [0.5, -0.5, 0.]],
            [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]],
            [[-1., 0., 0.], [1., 0., 0.], [-1., -1., 0.]],
        ] {
            let other = tri([3, 4, 5], p);
            assert!(intersects(&base, &other), "missed {p:?}");
            assert!(intersects(&other, &base));
        }
        let separated = tri(
            [3, 4, 5],
            [[0., 0., 0.001], [2., 0., 0.001], [0., 2., 0.001]],
        );
        assert!(!intersects(&base, &separated));
    }
    #[test]
    fn only_actual_shared_simplex_is_exempted() {
        let base = tri([0, 1, 2], [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]]);
        let valid_edge = tri([1, 0, 3], [[2., 0., 0.], [0., 0., 0.], [1., -1., 0.]]);
        let valid_vertex = tri([0, 3, 4], [[0., 0., 0.], [-1., 0., 1.], [0., -1., 1.]]);
        assert!(!intersects(&base, &valid_edge));
        assert!(!intersects(&base, &valid_vertex));
        let folded_edge = tri([1, 0, 3], [[2., 0., 0.], [0., 0., 0.], [0.5, 0.5, 0.]]);
        let vertex_crossing = tri([0, 3, 4], [[0., 0., 0.], [0.5, 0.5, -1.], [0.5, 0.5, 1.]]);
        assert!(intersects(&base, &folded_edge));
        let folded_sliver = tri([1, 0, 3], [[2., 0., 0.], [0., 0., 0.], [0.5, 1e-8, 0.]]);
        assert!(intersects(&base, &folded_sliver));
        assert!(intersects(&base, &vertex_crossing));
    }
    #[test]
    fn tolerance_scales_with_geometry() {
        for scale in [1e-8, 1e-3, 1., 1e6, 1e12] {
            let a = tri(
                [0, 1, 2],
                [[0., 0., 0.], [2. * scale, 0., 0.], [0., 2. * scale, 0.]],
            );
            let b = tri(
                [3, 4, 5],
                [
                    [scale / 2., scale / 2., -scale],
                    [scale / 2., scale / 2., scale],
                    [scale, scale / 2., 0.],
                ],
            );
            assert!(intersects(&a, &b), "scale {scale}");
        }
    }
    #[test]
    fn degeneracy_and_non_finite_values_fail_closed() {
        for p in [
            [DVec3::ZERO, DVec3::X, DVec3::X * 2.],
            [DVec3::ZERO, DVec3::X, DVec3::splat(f64::NAN)],
        ] {
            assert!(Triangle::new([0, 1, 2], p, ChunkId(0)).is_err());
        }
    }
    #[test]
    fn exhausted_geometric_work_budget_never_reports_success() {
        let a = tri([0, 1, 2], [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]);
        let b = tri([0, 3, 4], [[0., 0., 0.], [-1., 0., 0.], [0., -1., 0.]]);
        let triangles = vec![a, b];
        let mut indices = vec![0, 1];
        let bvh = Bvh::build(&triangles, &mut indices);
        assert_eq!(
            bvh.check_changed(0, &triangles[0], &triangles, &[true, true], &mut 0),
            Err(SafetyError::WorkLimit)
        );
        assert_eq!(
            motion_clear(
                triangles[0].p,
                triangles[0].p,
                triangles[1].p,
                triangles[1].p,
                1e-7,
                0,
                &mut 0
            ),
            Err(SafetyError::WorkLimit)
        );
    }

    #[test]
    fn swept_moving_pairs_are_checked_once_and_stationary_pairs_keep_the_work_cap() {
        // Disjoint coplanar triangles with overlapping AABBs. Their common
        // translation has a separating axis, so one pair consumes one proof.
        let triangles = [
            tri([0, 1, 2], [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]),
            tri([3, 4, 5], [[1., 1., 0.], [0.6, 1., 0.], [1., 0.6, 0.]]),
        ];
        let starts = triangles.each_ref().map(|triangle| triangle.p);
        let swept = triangles.map(|triangle| {
            let mut moving = Triangle::new(
                triangle.ids,
                triangle.p.map(|point| point + DVec3::Z * 0.1),
                triangle.chunk,
            )
            .unwrap();
            moving.min = moving.min.min(triangle.min);
            moving.max = moving.max.max(triangle.max);
            moving
        });
        let bvh = Bvh::build(&swept, &mut [0, 1]);
        let mut remaining = 1;
        for index in 0..2 {
            bvh.motion_candidates(index, &swept[index], &swept, &starts, &mut remaining)
                .unwrap();
        }
        assert_eq!(remaining, 0, "moving pair must be checked exactly once");

        let mut one_moving = swept;
        one_moving[0] = tri([0, 1, 2], starts[0].map(|point| point.to_array()));
        let bvh = Bvh::build(&one_moving, &mut [0, 1]);
        assert_eq!(
            bvh.motion_candidates(1, &one_moving[1], &one_moving, &starts, &mut 0),
            Err(SafetyError::WorkLimit),
            "an earlier stationary pair must not pass an exhausted proof budget"
        );
    }
}
