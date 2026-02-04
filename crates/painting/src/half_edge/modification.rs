//! Modification methods for HalfEdgeMesh.

use bevy::prelude::*;
use std::collections::HashMap;
use tracing::trace;

use super::types::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};
use super::HalfEdgeMesh;

/// Result of mesh compaction - maps old IDs to new IDs.
///
/// After edge collapse, dead elements (orphaned half-edges, removed faces,
/// disconnected vertices) remain in the arrays. Compaction removes them and
/// remaps all IDs to be contiguous.
///
/// Modeled after SculptGL's `applyDeletion()` which uses swap-and-pop to
/// remove dead triangles/vertices after decimation.
#[derive(Debug)]
pub struct CompactionMap {
    pub vertex_map: HashMap<VertexId, VertexId>,
    pub half_edge_map: HashMap<HalfEdgeId, HalfEdgeId>,
    pub face_map: HashMap<FaceId, FaceId>,
}

impl HalfEdgeMesh {
    /// Set the position of a vertex
    pub fn set_vertex_position(&mut self, vertex_id: VertexId, position: Vec3) {
        if let Some(v) = self.vertex_mut(vertex_id) {
            v.position = position;
        }
    }

    /// Flip an edge by swapping the diagonal of the two adjacent triangles.
    ///
    /// For an edge AB shared by triangles ABC and ABD, flipping creates
    /// triangles ACD and BCD (swapping the shared edge from AB to CD).
    ///
    /// ```text
    ///     Before:              After:
    ///        C                    C
    ///       /|\                  /|\
    ///      / | \                / | \
    ///     /  |  \              /  |  \
    ///    A---+---B    ->      A   |   B
    ///     \  |  /              \  |  /
    ///      \ | /                \ | /
    ///       \|/                  \|/
    ///        D                    D
    ///
    /// Edge AB becomes edge CD
    /// ```
    ///
    /// # Reference
    /// Adapted from SculptGL Decimation.js edge flip fallback:
    /// When 3+ shared neighbors exist, flipping is safer than collapsing.
    ///
    /// # Returns
    /// - `true` if the edge was successfully flipped
    /// - `false` if the flip could not be performed (boundary edge, invalid topology)
    ///
    /// # Preconditions
    /// - Edge must have a twin (interior edge, not boundary)
    /// - Both adjacent faces must be triangles
    pub fn flip_edge_topology(&mut self, edge_id: HalfEdgeId) -> bool {
        // ===== PHASE 1: GATHER (read-only, fail early) =====
        let he = match self.half_edge(edge_id) {
            Some(h) => h.clone(),
            None => return false,
        };

        // Edge flip requires a twin (can't flip boundary edges)
        let twin_id = match he.twin {
            Some(t) => t,
            None => return false,
        };

        let twin = match self.half_edge(twin_id) {
            Some(t) => t.clone(),
            None => return false,
        };

        // Get the face IDs
        let face1 = match he.face {
            Some(f) => f,
            None => return false,
        };
        let face2 = match twin.face {
            Some(f) => f,
            None => return false,
        };

        // Get all vertices involved
        // Face 1 (ABC): edge goes from A to B
        let v_a = he.origin;
        let v_b = match self.half_edge(he.next) {
            Some(n) => n.origin,
            None => return false,
        };
        let v_c = match self.half_edge(he.prev) {
            Some(p) => p.origin,
            None => return false,
        };

        // Face 2 (BAD): twin edge goes from B to A
        let v_d = match self.half_edge(twin.prev) {
            Some(p) => p.origin,
            None => return false,
        };

        // Get all half-edge IDs for rewiring
        let he_ab = edge_id;
        let he_bc = he.next;
        let he_ca = he.prev;

        let he_ba = twin_id;
        let he_ad = twin.next;
        let he_db = twin.prev;

        // ===== PHASE 2: VALIDATE =====
        // Verify both faces are triangles
        let face1_verts = self.get_face_vertices(face1);
        let face2_verts = self.get_face_vertices(face2);
        if face1_verts.len() != 3 || face2_verts.len() != 3 {
            return false;
        }

        // ===== PHASE 2b: NON-MANIFOLD PRE-CHECK =====
        // Flipping edge A-B creates edge C-D. If C→D or D→C already exists
        // from a DIFFERENT face, the flip would silently overwrite that face's
        // edge_map entry, creating a "shadow face" with a duplicate directed
        // edge. This produces non-manifold geometry that causes mesh tearing.
        for &(from, to) in &[(v_d, v_c), (v_c, v_d)] {
            if let Some(&existing_he) = self.edge_map.get(&(from, to)) {
                let existing_face = self.half_edges[existing_he.0 as usize].face;
                if let Some(ef) = existing_face {
                    if ef != face1 && ef != face2 {
                        trace!(
                            "flip_edge_topology: ABORT - new edge ({:?}->{:?}) already \
                             owned by face {:?}, would create non-manifold geometry",
                            from, to, ef
                        );
                        return false;
                    }
                }
            }
        }

        // ===== PHASE 3: REWIRE HALF-EDGES =====
        // After flip:
        // Face 1 becomes ADC: he_ad(A→D) → he_dc(D→C) → he_ca(C→A)
        // Face 2 becomes BCD: he_bc(B→C) → he_cd(C→D) → he_db(D→B)

        // Update he_ab to become he_dc (D → C)
        self.half_edges[he_ab.0 as usize].origin = v_d;
        self.half_edges[he_ab.0 as usize].next = he_ca;
        self.half_edges[he_ab.0 as usize].prev = he_ad;
        self.half_edges[he_ab.0 as usize].face = Some(face1);

        // Update he_ba to become he_cd (C → D)
        self.half_edges[he_ba.0 as usize].origin = v_c;
        self.half_edges[he_ba.0 as usize].next = he_db;
        self.half_edges[he_ba.0 as usize].prev = he_bc;
        self.half_edges[he_ba.0 as usize].face = Some(face2);

        // Update he_bc: moves to face2, between he_db and he_cd
        self.half_edges[he_bc.0 as usize].face = Some(face2);
        self.half_edges[he_bc.0 as usize].prev = he_db;
        self.half_edges[he_bc.0 as usize].next = he_ba; // he_cd

        // Update he_ca: stays in face1, between he_dc and he_ad
        self.half_edges[he_ca.0 as usize].prev = he_ab; // he_dc
        self.half_edges[he_ca.0 as usize].next = he_ad;

        // Update he_ad: moves to face1, between he_ca and he_dc
        self.half_edges[he_ad.0 as usize].face = Some(face1);
        self.half_edges[he_ad.0 as usize].prev = he_ca;
        self.half_edges[he_ad.0 as usize].next = he_ab; // he_dc

        // Update he_db: stays in face2, between he_cd and he_bc
        self.half_edges[he_db.0 as usize].face = Some(face2);
        self.half_edges[he_db.0 as usize].prev = he_ba; // he_cd
        self.half_edges[he_db.0 as usize].next = he_bc;

        // ===== PHASE 4: UPDATE FACES =====
        self.faces[face1.0 as usize].half_edge = he_ab; // he_cd
        self.faces[face2.0 as usize].half_edge = he_ba; // he_dc

        // ===== PHASE 5: UPDATE EDGE MAP =====
        self.edge_map.remove(&(v_a, v_b));
        self.edge_map.remove(&(v_b, v_a));
        self.edge_map.insert((v_d, v_c), he_ab); // he_ab is now D→C
        self.edge_map.insert((v_c, v_d), he_ba); // he_ba is now C→D

        // ===== PHASE 6: UPDATE VERTEX OUTGOING EDGES =====
        // A and B may have had their outgoing edge pointing to the flipped edge
        // Update them to point to valid outgoing edges
        if self.vertices[v_a.0 as usize].outgoing_half_edge == Some(he_ab) {
            self.vertices[v_a.0 as usize].outgoing_half_edge = Some(he_ad);
        }
        if self.vertices[v_b.0 as usize].outgoing_half_edge == Some(he_ba) {
            self.vertices[v_b.0 as usize].outgoing_half_edge = Some(he_bc);
        }

        // Set outgoing edges for C and D if they don't have one or it's now invalid
        if self.vertices[v_c.0 as usize]
            .outgoing_half_edge
            .map_or(true, |e| {
                self.half_edge(e)
                    .map_or(true, |h| h.origin != v_c || h.face.is_none())
            })
        {
            self.vertices[v_c.0 as usize].outgoing_half_edge = Some(he_ba); // he_cd (C→D)
        }
        if self.vertices[v_d.0 as usize]
            .outgoing_half_edge
            .map_or(true, |e| {
                self.half_edge(e)
                    .map_or(true, |h| h.origin != v_d || h.face.is_none())
            })
        {
            self.vertices[v_d.0 as usize].outgoing_half_edge = Some(he_ab); // he_dc (D→C)
        }

        // ===== PHASE 7: RECALCULATE FACE NORMALS =====
        // Update normals for the modified faces
        let face1_verts = self.get_face_vertices(face1);
        if face1_verts.len() >= 3 {
            let p0 = self.vertices[face1_verts[0].0 as usize].position;
            let p1 = self.vertices[face1_verts[1].0 as usize].position;
            let p2 = self.vertices[face1_verts[2].0 as usize].position;
            self.faces[face1.0 as usize].normal = (p1 - p0).cross(p2 - p0).normalize_or_zero();
        }

        let face2_verts = self.get_face_vertices(face2);
        if face2_verts.len() >= 3 {
            let p0 = self.vertices[face2_verts[0].0 as usize].position;
            let p1 = self.vertices[face2_verts[1].0 as usize].position;
            let p2 = self.vertices[face2_verts[2].0 as usize].position;
            self.faces[face2.0 as usize].normal = (p1 - p0).cross(p2 - p0).normalize_or_zero();
        }

        trace!(
            "flip_edge_topology: flipped edge {:?} from ({:?}->{:?}) to ({:?}<->{:?})",
            edge_id,
            v_a,
            v_b,
            v_c,
            v_d
        );

        true
    }

    /// Update face normals after vertex positions change
    pub fn recalculate_face_normals(&mut self) {
        for i in 0..self.faces.len() {
            let verts = self.get_face_vertices(FaceId(i as u32));
            if verts.len() >= 3 {
                let p0 = self.vertices[verts[0].0 as usize].position;
                let p1 = self.vertices[verts[1].0 as usize].position;
                let p2 = self.vertices[verts[2].0 as usize].position;
                let normal = (p1 - p0).cross(p2 - p0).normalize_or_zero();
                self.faces[i].normal = normal;
            }
        }
    }

    /// Recalculate vertex normals from adjacent face normals
    pub fn recalculate_vertex_normals(&mut self) {
        for i in 0..self.vertices.len() {
            let faces = self.get_vertex_faces(VertexId(i as u32));
            if faces.is_empty() {
                continue;
            }

            let mut normal = Vec3::ZERO;
            for fid in &faces {
                if let Some(f) = self.face(*fid) {
                    normal += f.normal;
                }
            }
            self.vertices[i].normal = normal.normalize_or_zero();
        }
    }

    /// Add a new vertex to the mesh.
    ///
    /// Returns the ID of the newly created vertex.
    pub fn add_vertex(&mut self, position: Vec3, normal: Vec3, uv: Option<Vec2>) -> VertexId {
        let id = VertexId(self.vertices.len() as u32);
        self.vertices.push(Vertex {
            id,
            position,
            normal,
            uv,
            outgoing_half_edge: None,
            source_index: u32::MAX, // New vertex has no source
        });
        id
    }

    /// Split an edge, creating a new vertex at the midpoint and subdividing adjacent faces.
    ///
    /// For a triangle mesh, splitting edge AB in triangle ABC creates:
    /// - New vertex M at the midpoint of AB
    /// - Triangles AMC and MBC (replacing ABC)
    /// - If there's a twin face ABD, creates AMD and MBD (replacing ABD)
    ///
    /// Uses a two-pass approach following Blender's DynTopo pattern:
    /// 1. Gather all data upfront, fail early if anything is missing
    /// 2. Create all new elements (half-edges with placeholder twins)
    /// 3. Set twins after all half-edges exist
    /// 4. Update auxiliary structures
    ///
    /// Returns the new vertex ID and the IDs of all newly created faces.
    pub fn split_edge_topology(&mut self, edge_id: HalfEdgeId) -> Option<(VertexId, Vec<FaceId>)> {
        trace!("split_edge_topology: START edge_id={:?}", edge_id);

        // ===== PHASE 1: GATHER ALL DATA (read-only, fail early) =====
        let he = self.half_edge(edge_id)?;
        let v0_id = he.origin;
        let face_id = he.face?; // Primary face must exist
        let twin_id = he.twin;
        let next_id = he.next;
        let prev_id = he.prev;

        let v1_id = self.half_edge(next_id)?.origin;
        let v2_id = self.half_edge(prev_id)?.origin;

        trace!(
            "split_edge_topology: v0={:?}, v1={:?}, v2={:?}, face={:?}, twin={:?}",
            v0_id, v1_id, v2_id, face_id, twin_id
        );

        // Gather twin face data if it exists (all-or-nothing)
        let twin_data: Option<(HalfEdgeId, FaceId, HalfEdgeId, HalfEdgeId, VertexId)> =
            if let Some(tid) = twin_id {
                let twin_he = self.half_edge(tid)?;
                let twin_face = twin_he.face?;
                let twin_next = twin_he.next;
                let twin_prev = twin_he.prev;
                let v3_id = self.half_edge(twin_prev)?.origin;
                Some((tid, twin_face, twin_next, twin_prev, v3_id))
            } else {
                None
            };

        // Calculate midpoint attributes
        let v0 = self.vertex(v0_id)?;
        let v1 = self.vertex(v1_id)?;
        let mid_pos = (v0.position + v1.position) * 0.5;
        let mid_normal = (v0.normal + v1.normal).normalize_or_zero();
        let mid_uv = match (v0.uv, v1.uv) {
            (Some(uv0), Some(uv1)) => Some((uv0 + uv1) * 0.5),
            _ => None,
        };

        let face_normal = self.faces[face_id.0 as usize].normal;

        // ===== PHASE 2: CREATE ALL NEW ELEMENTS =====

        // Create the new midpoint vertex
        let mid_id = self.add_vertex(mid_pos, mid_normal, mid_uv);

        // Pre-calculate all new IDs before pushing anything
        let base_he_id = self.half_edges.len() as u32;
        let he_mid_v2_id = HalfEdgeId(base_he_id);
        let he_mid_v1_id = HalfEdgeId(base_he_id + 1);
        let he_v2_mid_id = HalfEdgeId(base_he_id + 2);

        // Reused IDs
        let he_v0_mid_id = edge_id;
        let he_v1_v2_id = next_id;

        // Twin-side IDs (only used if twin exists)
        let he_mid_v3_id = HalfEdgeId(base_he_id + 3);
        let he_mid_v0_id = HalfEdgeId(base_he_id + 4);
        let he_v3_mid_id = HalfEdgeId(base_he_id + 5);

        // Create new faces
        let new_face_id = FaceId(self.faces.len() as u32);
        self.faces.push(Face {
            id: new_face_id,
            half_edge: he_mid_v1_id,
            normal: face_normal,
        });

        let twin_new_face_id = if let Some((_, twin_face, ..)) = twin_data {
            let id = FaceId(self.faces.len() as u32);
            let twin_normal = self.faces[twin_face.0 as usize].normal;
            self.faces.push(Face {
                id,
                half_edge: he_mid_v0_id,
                normal: twin_normal,
            });
            Some(id)
        } else {
            None
        };

        // ===== PHASE 3: CREATE ALL HALF-EDGES (cross-edge twins = None initially) =====

        // Primary face half-edges:
        // he_mid_v2: mid -> v2 (in original face, twin of he_v2_mid)
        self.half_edges.push(HalfEdge {
            id: he_mid_v2_id,
            origin: mid_id,
            twin: Some(he_v2_mid_id), // Internal twin - safe to set now
            next: prev_id,
            prev: he_v0_mid_id,
            face: Some(face_id),
        });

        // he_mid_v1: mid -> v1 (in new face) - twin crosses to twin face
        self.half_edges.push(HalfEdge {
            id: he_mid_v1_id,
            origin: mid_id,
            twin: None, // Will be set in Phase 4 if twin exists
            next: he_v1_v2_id,
            prev: he_v2_mid_id,
            face: Some(new_face_id),
        });

        // he_v2_mid: v2 -> mid (in new face, twin of he_mid_v2)
        self.half_edges.push(HalfEdge {
            id: he_v2_mid_id,
            origin: v2_id,
            twin: Some(he_mid_v2_id), // Internal twin - safe to set now
            next: he_mid_v1_id,
            prev: he_v1_v2_id,
            face: Some(new_face_id),
        });

        // Twin face half-edges (if exists)
        if let Some((twin_he_id, twin_face, twin_next_id, twin_prev_id, v3_id)) = twin_data {
            // he_mid_v3: mid -> v3 (in original twin face)
            self.half_edges.push(HalfEdge {
                id: he_mid_v3_id,
                origin: mid_id,
                twin: Some(he_v3_mid_id), // Internal twin - safe
                next: twin_prev_id,
                prev: twin_he_id, // he_v1_mid
                face: Some(twin_face),
            });

            // he_mid_v0: mid -> v0 (in new twin face) - twin crosses to primary face
            self.half_edges.push(HalfEdge {
                id: he_mid_v0_id,
                origin: mid_id,
                twin: None, // Will be set in Phase 4
                next: twin_next_id,
                prev: he_v3_mid_id,
                face: twin_new_face_id,
            });

            // he_v3_mid: v3 -> mid (in new twin face)
            self.half_edges.push(HalfEdge {
                id: he_v3_mid_id,
                origin: v3_id,
                twin: Some(he_mid_v3_id), // Internal twin - safe
                next: he_mid_v0_id,
                prev: twin_next_id,
                face: twin_new_face_id,
            });
        }

        // ===== PHASE 4: UPDATE EXISTING HALF-EDGES & SET CROSS-EDGE TWINS =====

        // Update he_v0_mid (reused edge_id): v0 -> mid
        self.half_edges[he_v0_mid_id.0 as usize].next = he_mid_v2_id;

        // Update he_v1_v2 (reused next_id): v1 -> v2, moves to new face
        self.half_edges[he_v1_v2_id.0 as usize].face = Some(new_face_id);
        self.half_edges[he_v1_v2_id.0 as usize].next = he_v2_mid_id;
        self.half_edges[he_v1_v2_id.0 as usize].prev = he_mid_v1_id;

        // Update prev (v2 -> v0): update prev to point to mid_v2
        self.half_edges[prev_id.0 as usize].prev = he_mid_v2_id;

        if let Some((twin_he_id, _, twin_next_id, twin_prev_id, _)) = twin_data {
            // Update he_v1_mid (reused twin_he_id): v1 -> mid
            self.half_edges[twin_he_id.0 as usize].next = he_mid_v3_id;

            // Update he_v0_v3 (reused twin_next_id): v0 -> v3, moves to new twin face
            self.half_edges[twin_next_id.0 as usize].face = twin_new_face_id;
            self.half_edges[twin_next_id.0 as usize].next = he_v3_mid_id;
            self.half_edges[twin_next_id.0 as usize].prev = he_mid_v0_id;

            // Update twin_prev (v3 -> v1): update prev
            self.half_edges[twin_prev_id.0 as usize].prev = he_mid_v3_id;

            // NOW set cross-edge twins (all half-edges exist now)
            self.half_edges[he_mid_v1_id.0 as usize].twin = Some(twin_he_id); // mid->v1 <-> v1->mid
            self.half_edges[twin_he_id.0 as usize].twin = Some(he_mid_v1_id);

            self.half_edges[he_v0_mid_id.0 as usize].twin = Some(he_mid_v0_id); // v0->mid <-> mid->v0
            self.half_edges[he_mid_v0_id.0 as usize].twin = Some(he_v0_mid_id);
        } else {
            // No twin face - he_v0_mid becomes boundary edge
            self.half_edges[he_v0_mid_id.0 as usize].twin = None;
            // he_mid_v1 already has twin = None
        }

        // ===== PHASE 5: UPDATE AUXILIARY STRUCTURES =====

        // Set outgoing half-edge for the new vertex
        self.vertices[mid_id.0 as usize].outgoing_half_edge = Some(he_mid_v2_id);

        // Update edge map for primary face
        self.edge_map.remove(&(v0_id, v1_id));
        self.edge_map.insert((v0_id, mid_id), he_v0_mid_id);
        self.edge_map.insert((mid_id, v2_id), he_mid_v2_id);
        self.edge_map.insert((mid_id, v1_id), he_mid_v1_id);
        self.edge_map.insert((v2_id, mid_id), he_v2_mid_id);

        // Update edge map for twin face (if exists)
        if let Some((twin_he_id, _, _, _, v3_id)) = twin_data {
            self.edge_map.remove(&(v1_id, v0_id));
            self.edge_map.insert((v1_id, mid_id), twin_he_id); // he_v1_mid
            self.edge_map.insert((mid_id, v3_id), he_mid_v3_id);
            self.edge_map.insert((mid_id, v0_id), he_mid_v0_id);
            self.edge_map.insert((v3_id, mid_id), he_v3_mid_id);
        }

        // ===== PHASE 6: UPDATE FACE POINTERS =====
        //
        // CRITICAL: The original face (face_id) is now AMC, not ABC.
        // Its half_edge pointer must point to a valid half-edge in the NEW topology.
        // Without this update, face traversal will follow a corrupted path and
        // cause duplicate edges when the mesh is rebuilt.
        //
        // face_id (AMC): he_v0_mid -> he_mid_v2 -> prev(v2->v0) -> he_v0_mid
        self.faces[face_id.0 as usize].half_edge = he_v0_mid_id;

        // Similarly for twin face: twin_face is now one triangle of the split
        // twin_he (v1->mid) -> he_mid_v3 -> twin_prev(v3->v1) -> twin_he
        if let Some((twin_he_id, twin_face, _, _, _)) = twin_data {
            self.faces[twin_face.0 as usize].half_edge = twin_he_id;
        }

        // Build result
        let mut new_faces = vec![new_face_id];
        if let Some(tf) = twin_new_face_id {
            new_faces.push(tf);
        }

        trace!(
            "split_edge_topology: END created vertex {:?}, {} new faces",
            mid_id,
            new_faces.len()
        );
        Some((mid_id, new_faces))
    }

    /// Collapse an edge, merging the destination vertex into the origin vertex.
    ///
    /// The origin vertex (v0) is moved to the midpoint and the destination vertex (v1)
    /// is removed. The two faces adjacent to the collapsed edge are also removed.
    ///
    /// Returns the IDs of removed faces, or None if the collapse cannot be performed.
    pub fn collapse_edge_topology(&mut self, edge_id: HalfEdgeId) -> Option<Vec<FaceId>> {
        trace!("collapse_edge_topology: START edge_id={:?}", edge_id);

        let he = self.half_edge(edge_id)?;
        let v0_id = he.origin;
        let twin_id = he.twin;
        let face_id = he.face;
        let next_id = he.next;

        // Get destination vertex
        let next_he = self.half_edge(next_id)?;
        let v1_id = next_he.origin;
        trace!(
            "collapse_edge_topology: v0={:?}, v1={:?}, face={:?}, twin={:?}",
            v0_id,
            v1_id,
            face_id,
            twin_id
        );

        // Calculate midpoint
        let v0 = self.vertex(v0_id)?;
        let v1 = self.vertex(v1_id)?;
        let mid_pos = (v0.position + v1.position) * 0.5;
        let mid_normal = (v0.normal + v1.normal).normalize_or_zero();

        let mut removed_faces = Vec::new();
        let mut orphaned_half_edges = Vec::new();

        // Collect half-edges from primary face
        if let Some(fid) = face_id {
            removed_faces.push(fid);
            for he_id in self.get_face_half_edges(fid) {
                orphaned_half_edges.push(he_id);
            }
        }

        // Collect half-edges from twin face
        if let Some(twin_he_id) = twin_id {
            if let Some(twin_face) = self.half_edges[twin_he_id.0 as usize].face {
                removed_faces.push(twin_face);
                for he_id in self.get_face_half_edges(twin_face) {
                    if !orphaned_half_edges.contains(&he_id) {
                        orphaned_half_edges.push(he_id);
                    }
                }
            }
        }

        // Collect all vertices from orphaned half-edges that need outgoing edge updates.
        // This includes v0, v2 (third vertex of face ABC), and v3 (third vertex of face ABD)
        // but NOT v1 which is being removed.
        let mut vertices_to_fix: Vec<VertexId> = vec![v0_id];
        for &he_id in &orphaned_half_edges {
            let origin = self.half_edges[he_id.0 as usize].origin;
            if origin != v1_id && !vertices_to_fix.contains(&origin) {
                vertices_to_fix.push(origin);
            }
        }

        // Save twin IDs from orphaned edges BEFORE orphaning clears them.
        // These are needed later to find valid outgoing edges for vertices_to_fix.
        let saved_orphan_twins: Vec<(HalfEdgeId, Option<HalfEdgeId>)> = orphaned_half_edges
            .iter()
            .map(|&he_id| (he_id, self.half_edges[he_id.0 as usize].twin))
            .collect();

        // Walk v1's 1-ring BEFORE orphaning to collect outgoing half-edges and neighbors.
        // After orphaning, v1's ring may be broken (outgoing_half_edge might point to an
        // orphaned edge), so we must do this first.
        let mut v1_outgoing: Vec<HalfEdgeId> = Vec::new();
        let mut v1_neighbors: Vec<VertexId> = Vec::new();
        if let Some(start_he_id) = self.vertices[v1_id.0 as usize].outgoing_half_edge {
            let mut current = start_he_id;
            let mut visited = std::collections::HashSet::new();
            let mut iterations = 0;
            loop {
                iterations += 1;
                if iterations > 100 || visited.contains(&current) {
                    break;
                }
                visited.insert(current);

                let he = &self.half_edges[current.0 as usize];
                if he.face.is_some() && he.origin == v1_id {
                    v1_outgoing.push(current);
                    // Collect destination vertex (neighbor of v1)
                    let dest_origin = self.half_edges[he.next.0 as usize].origin;
                    if !v1_neighbors.contains(&dest_origin) {
                        v1_neighbors.push(dest_origin);
                    }
                }

                // Move to next outgoing half-edge: prev → twin
                let prev_he = &self.half_edges[he.prev.0 as usize];
                if let Some(twin) = prev_he.twin {
                    current = twin;
                } else {
                    break; // Boundary
                }

                if current == start_he_id {
                    break;
                }
            }
        }

        // Orphan all collected half-edges: disconnect twins and clear face references
        for &he_id in &orphaned_half_edges {
            // Remove from edge map
            if let Some(dest) = self.get_half_edge_dest(he_id) {
                let origin = self.half_edges[he_id.0 as usize].origin;
                self.edge_map.remove(&(origin, dest));
            }

            // Disconnect twin (make it a boundary edge)
            if let Some(twin) = self.half_edges[he_id.0 as usize].twin {
                self.half_edges[twin.0 as usize].twin = None;
            }

            // Clear this half-edge's face and twin references
            self.half_edges[he_id.0 as usize].face = None;
            self.half_edges[he_id.0 as usize].twin = None;
        }

        // Remove orphaned edges from v1_outgoing (they were orphaned above)
        v1_outgoing.retain(|he_id| self.half_edges[he_id.0 as usize].face.is_some());

        // Redirect v1's remaining (non-orphaned) outgoing half-edges to originate from v0 (O(valence))
        for &he_id in &v1_outgoing {
            self.half_edges[he_id.0 as usize].origin = v0_id;
        }

        // Update edge_map for redirected edges: remove (v1, dest) entries,
        // insert (v0, dest) entries (O(valence))
        for &he_id in &v1_outgoing {
            let dest = self.half_edges[self.half_edges[he_id.0 as usize].next.0 as usize].origin;
            self.edge_map.remove(&(v1_id, dest));
            if self.half_edges[he_id.0 as usize].face.is_some() {
                // Diagnostic: detect if we're about to overwrite a live edge_map entry.
                // This would create a "shadow face" and non-manifold geometry.
                if let Some(&existing) = self.edge_map.get(&(v0_id, dest)) {
                    if self.half_edges[existing.0 as usize].face.is_some() {
                        tracing::error!(
                            "collapse_edge_topology: OVERWRITE ({:?}->{:?}): \
                             existing he={:?} face={:?}, new he={:?}",
                            v0_id,
                            dest,
                            existing,
                            self.half_edges[existing.0 as usize].face,
                            he_id
                        );
                    }
                }
                self.edge_map.insert((v0_id, dest), he_id);
            }
        }

        // Update edge_map entries where destination was v1 (now v0).
        // For each neighbor X of v1, look up edge_map[(X, v1)] directly (O(valence) lookups).
        for &neighbor in &v1_neighbors {
            if let Some(he_id) = self.edge_map.remove(&(neighbor, v1_id)) {
                if self.half_edges[he_id.0 as usize].face.is_some() {
                    // Diagnostic: detect shadow face creation on incoming edges
                    if let Some(&existing) = self.edge_map.get(&(neighbor, v0_id)) {
                        if self.half_edges[existing.0 as usize].face.is_some() {
                            tracing::error!(
                                "collapse_edge_topology: OVERWRITE ({:?}->{:?}): \
                                 existing he={:?} face={:?}, new he={:?}",
                                neighbor,
                                v0_id,
                                existing,
                                self.half_edges[existing.0 as usize].face,
                                he_id
                            );
                        }
                    }
                    self.edge_map.insert((neighbor, v0_id), he_id);
                }
            }
        }

        // Detect degenerate half-edges created by the redirect (O(valence)).
        // Only check redirected edges — only they can have degenerate v0→v0 edges.
        let mut degenerate_faces: Vec<FaceId> = Vec::new();
        for &he_id in &v1_outgoing {
            let he = &self.half_edges[he_id.0 as usize];
            if he.face.is_some() {
                let dest = self.half_edges[he.next.0 as usize].origin;
                if dest == v0_id {
                    if let Some(fid) = he.face {
                        if !degenerate_faces.contains(&fid) {
                            degenerate_faces.push(fid);
                        }
                    }
                }
            }
        }

        if !degenerate_faces.is_empty() {
            trace!(
                "collapse_edge_topology: cleaning up {} degenerate faces",
                degenerate_faces.len()
            );
            for &fid in &degenerate_faces {
                for he_id in self.get_face_half_edges(fid) {
                    // Disconnect twin
                    if let Some(twin) = self.half_edges[he_id.0 as usize].twin {
                        self.half_edges[twin.0 as usize].twin = None;
                    }
                    // Remove from edge map
                    if let Some(dest) = self.get_half_edge_dest(he_id) {
                        let origin = self.half_edges[he_id.0 as usize].origin;
                        self.edge_map.remove(&(origin, dest));
                    }
                    self.half_edges[he_id.0 as usize].face = None;
                    self.half_edges[he_id.0 as usize].twin = None;
                }
                removed_faces.push(fid);
            }
        }

        // Move v0 to midpoint
        self.vertices[v0_id.0 as usize].position = mid_pos;
        self.vertices[v0_id.0 as usize].normal = mid_normal;

        // Fix outgoing_half_edge for affected vertices (O(valence) per vertex).
        // Uses saved twin info and redirected edges — no O(E) scans.
        for vid in vertices_to_fix {
            // First check if existing outgoing edge is still valid
            if let Some(existing) = self.vertices[vid.0 as usize].outgoing_half_edge {
                let he = &self.half_edges[existing.0 as usize];
                if he.origin == vid && he.face.is_some() {
                    continue; // Still valid, no fix needed
                }
            }

            let mut found = None;

            if vid == v0_id {
                // For v0: use redirected edges from v1 (now originating from v0)
                for &he_id in &v1_outgoing {
                    let he = &self.half_edges[he_id.0 as usize];
                    if he.origin == v0_id && he.face.is_some() {
                        found = Some(he_id);
                        break;
                    }
                }
            }

            // For v2, v3, etc.: use saved twins from orphaned edges.
            // If an orphaned edge went INTO vid (its next.origin == vid), then
            // the twin of that orphaned edge originates from vid and is still live.
            if found.is_none() {
                for &(_orphaned_id, twin_opt) in &saved_orphan_twins {
                    if let Some(twin_id) = twin_opt {
                        let twin_he = &self.half_edges[twin_id.0 as usize];
                        if twin_he.origin == vid && twin_he.face.is_some() {
                            found = Some(twin_id);
                            break;
                        }
                    }
                }
            }

            self.vertices[vid.0 as usize].outgoing_half_edge = found;
        }

        // Mark v1 as having no outgoing edge (effectively removed)
        self.vertices[v1_id.0 as usize].outgoing_half_edge = None;

        trace!(
            "collapse_edge_topology: END removed {} faces ({} degenerate), {} orphaned half-edges",
            removed_faces.len(),
            degenerate_faces.len(),
            orphaned_half_edges.len()
        );
        Some(removed_faces)
    }

    /// Remove dead faces, half-edges, and vertices from arrays and remap all IDs.
    ///
    /// After edge collapse, elements are orphaned but not removed from arrays.
    /// This method compacts the arrays by removing dead elements and updating
    /// all cross-references, equivalent to SculptGL's `applyDeletion()`.
    ///
    /// Uses **reachability-based liveness**: walks from faces → half-edges → vertices.
    /// This guarantees consistency: no live half-edge can reference a dead vertex.
    ///
    /// Also detects and excludes degenerate faces (faces with duplicate vertices,
    /// which arise from non-manifold geometry after collapse).
    ///
    /// # Returns
    /// A `CompactionMap` mapping old IDs to new IDs for all three element types.
    pub fn compact(&mut self) -> CompactionMap {
        use std::collections::HashSet;

        let compact_start = std::time::Instant::now();
        tracing::debug!(
            "compact: START ({} verts, {} half-edges, {} faces)",
            self.vertices.len(),
            self.half_edges.len(),
            self.faces.len()
        );

        // Phase 1a: Identify live faces
        // A face is live if its half_edge points to a half-edge that belongs to it
        let mut live_face_ids: HashSet<FaceId> = HashSet::new();
        for (i, f) in self.faces.iter().enumerate() {
            let face_id = FaceId(i as u32);
            let is_live = self
                .half_edges
                .get(f.half_edge.0 as usize)
                .map_or(false, |he| he.face == Some(face_id));
            if is_live {
                live_face_ids.insert(face_id);
            }
        }

        // Phase 1b: Walk live faces to collect live half-edges and detect degenerate faces.
        // A degenerate face has duplicate vertices (e.g., from non-manifold collapse).
        let mut live_he_ids: HashSet<HalfEdgeId> = HashSet::new();
        let mut degenerate_face_ids: HashSet<FaceId> = HashSet::new();

        for &face_id in &live_face_ids {
            let face = &self.faces[face_id.0 as usize];
            let start = face.half_edge;
            let mut current = start;
            let mut face_vertices: Vec<VertexId> = Vec::with_capacity(3);
            let mut face_hes: Vec<HalfEdgeId> = Vec::with_capacity(3);
            let mut is_degenerate = false;

            loop {
                let he = &self.half_edges[current.0 as usize];
                // Check for duplicate vertex (degenerate face)
                if face_vertices.contains(&he.origin) {
                    is_degenerate = true;
                    break;
                }
                face_vertices.push(he.origin);
                face_hes.push(current);
                current = he.next;
                if current == start || face_hes.len() > 6 {
                    break;
                }
            }

            if is_degenerate {
                degenerate_face_ids.insert(face_id);
            } else {
                for he_id in face_hes {
                    live_he_ids.insert(he_id);
                }
            }
        }

        // Remove degenerate faces from live set
        for fid in &degenerate_face_ids {
            live_face_ids.remove(fid);
        }

        if !degenerate_face_ids.is_empty() {
            trace!(
                "compact: found {} degenerate faces to remove",
                degenerate_face_ids.len()
            );
        }

        // Phase 1b-extra: Detect non-manifold edges (duplicate directed edges across faces).
        // This catches "shadow faces" created when flip_edge_topology overwrites an existing
        // edge_map entry. Both faces are live but share a directed edge, which violates
        // manifold topology and causes mesh tearing during merge.
        {
            let mut temp_edge_owners: HashMap<(VertexId, VertexId), FaceId> =
                HashMap::with_capacity(live_he_ids.len());
            let mut nonmanifold_face_ids: Vec<FaceId> = Vec::new();

            for &face_id in &live_face_ids {
                if degenerate_face_ids.contains(&face_id) {
                    continue;
                }
                let face = &self.faces[face_id.0 as usize];
                let start = face.half_edge;
                let mut current = start;
                let mut is_nonmanifold = false;
                loop {
                    let he = &self.half_edges[current.0 as usize];
                    let dest = self.half_edges[he.next.0 as usize].origin;
                    if let Some(&earlier_face) = temp_edge_owners.get(&(he.origin, dest)) {
                        tracing::warn!(
                            "compact: NON-MANIFOLD directed edge ({:?}->{:?}) shared by \
                             face {:?} and {:?}. Removing face {:?}.",
                            he.origin, dest, earlier_face, face_id, face_id
                        );
                        is_nonmanifold = true;
                        break;
                    }
                    temp_edge_owners.insert((he.origin, dest), face_id);
                    current = he.next;
                    if current == start {
                        break;
                    }
                }
                if is_nonmanifold {
                    nonmanifold_face_ids.push(face_id);
                }
            }

            // Remove non-manifold faces and their half-edges from live sets
            for &fid in &nonmanifold_face_ids {
                live_face_ids.remove(&fid);
                degenerate_face_ids.insert(fid);

                // Remove this face's half-edges from live_he_ids
                let face = &self.faces[fid.0 as usize];
                let start = face.half_edge;
                let mut current = start;
                loop {
                    live_he_ids.remove(&current);
                    current = self.half_edges[current.0 as usize].next;
                    if current == start {
                        break;
                    }
                }
            }

            if !nonmanifold_face_ids.is_empty() {
                tracing::warn!(
                    "compact: removed {} non-manifold faces (duplicate directed edges)",
                    nonmanifold_face_ids.len()
                );
            }
        }

        // Phase 1c: From live half-edges, collect live vertices
        let mut live_vertex_ids: HashSet<VertexId> = HashSet::new();
        for &he_id in &live_he_ids {
            let he = &self.half_edges[he_id.0 as usize];
            live_vertex_ids.insert(he.origin);
        }

        // Phase 2: Build remapping tables
        let mut vertex_map: HashMap<VertexId, VertexId> = HashMap::new();
        let mut half_edge_map: HashMap<HalfEdgeId, HalfEdgeId> = HashMap::new();
        let mut face_map: HashMap<FaceId, FaceId> = HashMap::new();

        let mut new_vertex_idx = 0u32;
        for i in 0..self.vertices.len() {
            let vid = VertexId(i as u32);
            if live_vertex_ids.contains(&vid) {
                vertex_map.insert(vid, VertexId(new_vertex_idx));
                new_vertex_idx += 1;
            }
        }

        // Phase 2b: Pre-filter half-edges before assigning IDs.
        // A live half-edge must have: live origin vertex, live next, live prev.
        // Iteratively exclude half-edges that fail these checks, since excluding
        // one can cascade to others (a half-edge whose next was excluded must also
        // be excluded). This prevents the old bug where `continue` in Phase 3
        // caused array index/ID misalignment.
        let mut included_he_ids = live_he_ids.clone();
        loop {
            let to_exclude: Vec<HalfEdgeId> = included_he_ids
                .iter()
                .filter(|&&he_id| {
                    let he = &self.half_edges[he_id.0 as usize];
                    let origin_ok = live_vertex_ids.contains(&he.origin);
                    let next_ok = included_he_ids.contains(&he.next);
                    let prev_ok = included_he_ids.contains(&he.prev);
                    !origin_ok || !next_ok || !prev_ok
                })
                .copied()
                .collect();
            if to_exclude.is_empty() {
                break;
            }
            for he_id in &to_exclude {
                let he = &self.half_edges[he_id.0 as usize];
                tracing::warn!(
                    "compact: excluding live half-edge {:?} (origin={:?}, next={:?}, prev={:?}) - dead references",
                    he_id, he.origin, he.next, he.prev
                );
                included_he_ids.remove(he_id);
            }
        }

        // Also exclude faces whose starting half-edge was excluded
        live_face_ids.retain(|&fid| {
            let face = &self.faces[fid.0 as usize];
            included_he_ids.contains(&face.half_edge)
        });

        // Build half-edge and face maps from the filtered sets
        let mut new_he_idx = 0u32;
        for i in 0..self.half_edges.len() {
            let he_id = HalfEdgeId(i as u32);
            if included_he_ids.contains(&he_id) {
                half_edge_map.insert(he_id, HalfEdgeId(new_he_idx));
                new_he_idx += 1;
            }
        }

        let mut new_face_idx = 0u32;
        for i in 0..self.faces.len() {
            let fid = FaceId(i as u32);
            if live_face_ids.contains(&fid) {
                face_map.insert(fid, FaceId(new_face_idx));
                new_face_idx += 1;
            }
        }

        let removed_verts = self.vertices.len() - vertex_map.len();
        let removed_hes = self.half_edges.len() - half_edge_map.len();
        let removed_faces = self.faces.len() - face_map.len();

        // Early exit if nothing to compact
        if removed_verts == 0 && removed_hes == 0 && removed_faces == 0 {
            return CompactionMap {
                vertex_map,
                half_edge_map,
                face_map,
            };
        }

        trace!(
            "compact: removing {} vertices, {} half-edges, {} faces ({} degenerate)",
            removed_verts,
            removed_hes,
            removed_faces,
            degenerate_face_ids.len()
        );

        // Phase 3: Build new arrays with remapped IDs.
        // Half-edges were pre-filtered in Phase 2b, so no defensive skipping needed here.
        // This guarantees that array indices always match half-edge IDs.
        let mut new_vertices: Vec<Vertex> = Vec::with_capacity(vertex_map.len());
        for (i, v) in self.vertices.iter().enumerate() {
            let vid = VertexId(i as u32);
            if let Some(&new_id) = vertex_map.get(&vid) {
                new_vertices.push(Vertex {
                    id: new_id,
                    position: v.position,
                    normal: v.normal,
                    uv: v.uv,
                    outgoing_half_edge: v
                        .outgoing_half_edge
                        .and_then(|he| half_edge_map.get(&he).copied()),
                    source_index: v.source_index,
                });
            }
        }

        let mut new_half_edges: Vec<HalfEdge> = Vec::with_capacity(half_edge_map.len());
        for (i, he) in self.half_edges.iter().enumerate() {
            let he_id = HalfEdgeId(i as u32);
            if let Some(&new_id) = half_edge_map.get(&he_id) {
                // All references are guaranteed valid by Phase 2b pre-filtering.
                let new_origin = vertex_map[&he.origin];
                let new_next = half_edge_map[&he.next];
                let new_prev = half_edge_map[&he.prev];

                new_half_edges.push(HalfEdge {
                    id: new_id,
                    origin: new_origin,
                    twin: he.twin.and_then(|t| half_edge_map.get(&t).copied()),
                    next: new_next,
                    prev: new_prev,
                    face: he.face.and_then(|f| face_map.get(&f).copied()),
                });
            }
        }

        // Sanity check: array length must match map size (guaranteed by pre-filtering)
        debug_assert_eq!(
            new_half_edges.len(),
            half_edge_map.len(),
            "compact: half-edge array/map size mismatch after pre-filtered build"
        );

        let mut new_faces: Vec<Face> = Vec::with_capacity(face_map.len());
        for (i, f) in self.faces.iter().enumerate() {
            let fid = FaceId(i as u32);
            if let Some(&new_id) = face_map.get(&fid) {
                let Some(&new_he) = half_edge_map.get(&f.half_edge) else {
                    trace!(
                        "compact: WARN face {:?} references dead half-edge {:?}, skipping",
                        fid, f.half_edge
                    );
                    continue;
                };
                new_faces.push(Face {
                    id: new_id,
                    half_edge: new_he,
                    normal: f.normal,
                });
            }
        }

        // Phase 3: Rebuild edge_map from scratch using live half-edges.
        // The old edge_map may have stale entries from collapse redirects where
        // the key (origin, dest) no longer matches the half-edge's actual direction.
        // Walking the new arrays guarantees correctness.
        let mut new_edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> =
            HashMap::with_capacity(new_half_edges.len());
        for he in &new_half_edges {
            if he.face.is_some() {
                let dest = new_half_edges[he.next.0 as usize].origin;
                new_edge_map.insert((he.origin, dest), he.id);
            }
        }

        // Phase 4: Replace arrays
        self.vertices = new_vertices;
        self.half_edges = new_half_edges;
        self.faces = new_faces;
        self.edge_map = new_edge_map;

        // Phase 5: Fix outgoing_half_edge for all vertices.
        // With reachability-based liveness, a vertex may be live (referenced by live
        // half-edges) but have outgoing_half_edge = None or pointing to a now-dead
        // half-edge. Scan all half-edges to assign valid outgoing edges.
        for he in self.half_edges.iter() {
            let vid = he.origin;
            let v = &mut self.vertices[vid.0 as usize];
            if v.outgoing_half_edge.is_none() {
                v.outgoing_half_edge = Some(he.id);
            }
        }

        // Phase 5b: Rebuild twin pointers from the fresh edge_map.
        // This guarantees twin consistency regardless of what the collapse code did.
        self.rebuild_twins_from_edge_map();

        tracing::debug!(
            "compact: END in {:?} ({} vertices, {} half-edges, {} faces)",
            compact_start.elapsed(),
            self.vertices.len(),
            self.half_edges.len(),
            self.faces.len()
        );

        CompactionMap {
            vertex_map,
            half_edge_map,
            face_map,
        }
    }

    /// Rebuild all twin pointers from the authoritative `edge_map`.
    ///
    /// After batch operations (multiple sequential splits), twin pointers may be
    /// inconsistent because each split rewires next/prev/twin/face on surrounding
    /// half-edges, and a later split may reference a half-edge whose pointers were
    /// already changed by an earlier split.
    ///
    /// The `edge_map` is always correctly maintained by `split_edge_topology()`
    /// (each split updates it with the new directed edges). This method rebuilds
    /// twin pointers from scratch using the edge_map as the source of truth.
    ///
    /// For each directed edge (v0→v1) with half-edge H, if the reverse edge
    /// (v1→v0) also exists with half-edge T, then H.twin = T and T.twin = H.
    pub fn rebuild_twins_from_edge_map(&mut self) {
        // Clear all twin pointers on live half-edges
        for he in self.half_edges.iter_mut() {
            if he.face.is_some() {
                he.twin = None;
            }
        }

        // Rebuild from edge_map: for each (v0,v1)->he, look up (v1,v0)
        let pairs: Vec<_> = self.edge_map.iter().map(|(&k, &v)| (k, v)).collect();
        for ((v0, v1), he_id) in &pairs {
            if let Some(&twin_id) = self.edge_map.get(&(*v1, *v0)) {
                self.half_edges[he_id.0 as usize].twin = Some(twin_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::half_edge::HalfEdgeMesh;

    /// Create a simple mesh with two triangles sharing an edge:
    ///
    /// ```text
    ///     v2
    ///    /|\
    ///   / | \
    ///  /  |  \
    /// v0--+--v1
    ///  \  |  /
    ///   \ | /
    ///    \|/
    ///     v3
    /// ```
    ///
    /// Triangles: (v0, v1, v2) and (v1, v0, v3)
    /// The shared edge is v0 -> v1
    fn create_bowtie_mesh() -> HalfEdgeMesh {
        use crate::half_edge::types::{Face, HalfEdge, Vertex};
        use std::collections::HashMap;

        let vertices = vec![
            Vertex {
                id: VertexId(0),
                position: Vec3::new(-1.0, 0.0, 0.0),
                normal: Vec3::Y,
                uv: None,
                outgoing_half_edge: Some(HalfEdgeId(0)),
                source_index: 0,
            },
            Vertex {
                id: VertexId(1),
                position: Vec3::new(1.0, 0.0, 0.0),
                normal: Vec3::Y,
                uv: None,
                outgoing_half_edge: Some(HalfEdgeId(1)),
                source_index: 1,
            },
            Vertex {
                id: VertexId(2),
                position: Vec3::new(0.0, 1.0, 0.0),
                normal: Vec3::Y,
                uv: None,
                outgoing_half_edge: Some(HalfEdgeId(2)),
                source_index: 2,
            },
            Vertex {
                id: VertexId(3),
                position: Vec3::new(0.0, -1.0, 0.0),
                normal: Vec3::Y,
                uv: None,
                outgoing_half_edge: Some(HalfEdgeId(5)),
                source_index: 3,
            },
        ];

        // Face 0: v0 -> v1 -> v2 (counter-clockwise)
        // Half-edges: 0 (v0->v1), 1 (v1->v2), 2 (v2->v0)
        //
        // Face 1: v1 -> v0 -> v3 (counter-clockwise)
        // Half-edges: 3 (v1->v0), 4 (v0->v3), 5 (v3->v1)
        let half_edges = vec![
            // Face 0
            HalfEdge {
                id: HalfEdgeId(0),
                origin: VertexId(0),
                twin: Some(HalfEdgeId(3)),
                next: HalfEdgeId(1),
                prev: HalfEdgeId(2),
                face: Some(FaceId(0)),
            },
            HalfEdge {
                id: HalfEdgeId(1),
                origin: VertexId(1),
                twin: None,
                next: HalfEdgeId(2),
                prev: HalfEdgeId(0),
                face: Some(FaceId(0)),
            },
            HalfEdge {
                id: HalfEdgeId(2),
                origin: VertexId(2),
                twin: None,
                next: HalfEdgeId(0),
                prev: HalfEdgeId(1),
                face: Some(FaceId(0)),
            },
            // Face 1
            HalfEdge {
                id: HalfEdgeId(3),
                origin: VertexId(1),
                twin: Some(HalfEdgeId(0)),
                next: HalfEdgeId(4),
                prev: HalfEdgeId(5),
                face: Some(FaceId(1)),
            },
            HalfEdge {
                id: HalfEdgeId(4),
                origin: VertexId(0),
                twin: None,
                next: HalfEdgeId(5),
                prev: HalfEdgeId(3),
                face: Some(FaceId(1)),
            },
            HalfEdge {
                id: HalfEdgeId(5),
                origin: VertexId(3),
                twin: None,
                next: HalfEdgeId(3),
                prev: HalfEdgeId(4),
                face: Some(FaceId(1)),
            },
        ];

        let faces = vec![
            Face {
                id: FaceId(0),
                half_edge: HalfEdgeId(0),
                normal: Vec3::Z,
            },
            Face {
                id: FaceId(1),
                half_edge: HalfEdgeId(3),
                normal: Vec3::NEG_Z,
            },
        ];

        let mut edge_map = HashMap::new();
        edge_map.insert((VertexId(0), VertexId(1)), HalfEdgeId(0));
        edge_map.insert((VertexId(1), VertexId(2)), HalfEdgeId(1));
        edge_map.insert((VertexId(2), VertexId(0)), HalfEdgeId(2));
        edge_map.insert((VertexId(1), VertexId(0)), HalfEdgeId(3));
        edge_map.insert((VertexId(0), VertexId(3)), HalfEdgeId(4));
        edge_map.insert((VertexId(3), VertexId(1)), HalfEdgeId(5));

        HalfEdgeMesh::from_raw(vertices, half_edges, faces, edge_map)
    }

    #[test]
    fn test_split_edge_increases_face_count() {
        let mut mesh = create_bowtie_mesh();
        assert_eq!(mesh.face_count(), 2);
        assert_eq!(mesh.vertex_count(), 4);

        // Split the shared edge (v0 -> v1, half-edge 0)
        let result = mesh.split_edge_topology(HalfEdgeId(0));
        assert!(result.is_some());

        let (_new_vertex, new_faces) = result.unwrap();

        // Should have created 1 new vertex (midpoint)
        assert_eq!(mesh.vertex_count(), 5);

        // Should have created 2 new faces (one on each side of split)
        assert_eq!(new_faces.len(), 2);
        assert_eq!(mesh.face_count(), 4); // 2 original + 2 new
    }

    #[test]
    fn test_split_edge_midpoint_position() {
        let mut mesh = create_bowtie_mesh();

        let v0_pos = mesh.vertex(VertexId(0)).unwrap().position;
        let v1_pos = mesh.vertex(VertexId(1)).unwrap().position;
        let expected_mid = (v0_pos + v1_pos) * 0.5;

        let result = mesh.split_edge_topology(HalfEdgeId(0));
        let (new_vertex_id, _) = result.unwrap();

        let mid_pos = mesh.vertex(new_vertex_id).unwrap().position;
        assert!((mid_pos - expected_mid).length() < 0.001);
    }

    #[test]
    fn test_split_edge_connectivity_valid() {
        let mut mesh = create_bowtie_mesh();

        mesh.split_edge_topology(HalfEdgeId(0));

        // Validate connectivity is intact
        let validation = mesh.validate_connectivity();
        assert!(
            validation.is_ok(),
            "Mesh connectivity invalid after split: {:?}",
            validation.err()
        );
    }

    #[test]
    fn test_split_edge_all_faces_traversable() {
        let mut mesh = create_bowtie_mesh();

        mesh.split_edge_topology(HalfEdgeId(0));

        // Every face should be traversable with get_face_vertices
        for i in 0..mesh.face_count() {
            let verts = mesh.get_face_vertices(FaceId(i as u32));
            assert_eq!(
                verts.len(),
                3,
                "Face {} should have 3 vertices, got {}",
                i,
                verts.len()
            );
        }
    }

    #[test]
    fn test_split_edge_twin_symmetry() {
        let mut mesh = create_bowtie_mesh();

        mesh.split_edge_topology(HalfEdgeId(0));

        // Check all twins are symmetric
        for he in mesh.half_edges() {
            if he.face.is_none() {
                continue; // Skip orphaned
            }
            if let Some(twin_id) = he.twin {
                let twin = mesh.half_edge(twin_id).unwrap();
                assert_eq!(
                    twin.twin,
                    Some(he.id),
                    "Twin symmetry violated: {:?} -> {:?} but {:?} -> {:?}",
                    he.id,
                    twin_id,
                    twin_id,
                    twin.twin
                );
            }
        }
    }

    #[test]
    fn test_split_edge_on_boundary() {
        let mut mesh = create_bowtie_mesh();

        // Split edge v1 -> v2 (half-edge 1), which has no twin
        let result = mesh.split_edge_topology(HalfEdgeId(1));
        assert!(result.is_some());

        let (_, new_faces) = result.unwrap();

        // Should only create 1 new face (no twin face to split)
        assert_eq!(new_faces.len(), 1);
        assert_eq!(mesh.face_count(), 3); // 2 original + 1 new

        // Connectivity should still be valid
        assert!(mesh.validate_connectivity().is_ok());
    }

    #[test]
    fn test_compact_no_dead_elements() {
        let mut mesh = create_bowtie_mesh();
        let original_verts = mesh.vertex_count();
        let original_faces = mesh.face_count();
        let original_hes = mesh.half_edges().len();

        let compaction = mesh.compact();

        // No dead elements, so nothing should change
        assert_eq!(mesh.vertex_count(), original_verts);
        assert_eq!(mesh.face_count(), original_faces);
        assert_eq!(mesh.half_edges().len(), original_hes);

        // All IDs should map to themselves
        assert_eq!(compaction.vertex_map.len(), original_verts);
        assert_eq!(compaction.face_map.len(), original_faces);
        assert_eq!(compaction.half_edge_map.len(), original_hes);
    }

    #[test]
    fn test_compact_after_collapse() {
        let mut mesh = create_bowtie_mesh();
        assert_eq!(mesh.face_count(), 2);
        assert_eq!(mesh.vertex_count(), 4);

        // First split to create a mesh with 4 faces, then collapse
        mesh.split_edge_topology(HalfEdgeId(0));
        assert_eq!(mesh.face_count(), 4);
        assert_eq!(mesh.vertex_count(), 5);

        // Before compact: collapse will leave dead elements
        let pre_compact_faces = mesh.face_count();
        let pre_compact_verts = mesh.vertex_count();

        // Collapse an edge - this orphans faces but doesn't remove them
        let result = mesh.collapse_edge_topology(HalfEdgeId(0));
        assert!(result.is_some());

        // Face count is unchanged (dead faces remain in array)
        assert_eq!(mesh.face_count(), pre_compact_faces);

        // Compact should remove dead elements
        let compaction = mesh.compact();

        // After compact, dead elements should be gone
        assert!(mesh.face_count() < pre_compact_faces);
        assert!(mesh.vertex_count() < pre_compact_verts);

        // All remaining faces should be valid
        for i in 0..mesh.face_count() {
            assert!(
                mesh.is_face_valid(FaceId(i as u32)),
                "Face {} should be valid after compact",
                i
            );
            let verts = mesh.get_face_vertices(FaceId(i as u32));
            assert_eq!(
                verts.len(),
                3,
                "Face {} should have 3 vertices after compact, got {}",
                i,
                verts.len()
            );
        }

        // Vertex map should only contain live vertices
        assert_eq!(compaction.vertex_map.len(), mesh.vertex_count());
    }
}
