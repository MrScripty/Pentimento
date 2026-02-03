//! Modification methods for HalfEdgeMesh.

use bevy::prelude::*;
use tracing::trace;

use super::types::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};
use super::HalfEdgeMesh;

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

        // ===== PHASE 3: REWIRE HALF-EDGES =====
        // After flip:
        // Face 1 becomes ACD: edge goes from C to D
        // Face 2 becomes DBC: twin edge goes from D to C

        // Update he_ab to become he_cd (C -> D)
        self.half_edges[he_ab.0 as usize].origin = v_c;
        self.half_edges[he_ab.0 as usize].next = he_db;
        self.half_edges[he_ab.0 as usize].prev = he_ca;
        self.half_edges[he_ab.0 as usize].face = Some(face1);

        // Update he_ba to become he_dc (D -> C)
        self.half_edges[he_ba.0 as usize].origin = v_d;
        self.half_edges[he_ba.0 as usize].next = he_bc;
        self.half_edges[he_ba.0 as usize].prev = he_ad;
        self.half_edges[he_ba.0 as usize].face = Some(face2);

        // Update he_bc: now in face2, prev is he_dc, next is he_ca→he_ad
        self.half_edges[he_bc.0 as usize].face = Some(face2);
        self.half_edges[he_bc.0 as usize].prev = he_ba; // he_dc
        self.half_edges[he_bc.0 as usize].next = he_ad;

        // Update he_ca: now stays in face1, prev is he_db, next is he_cd
        self.half_edges[he_ca.0 as usize].prev = he_db;
        self.half_edges[he_ca.0 as usize].next = he_ab; // he_cd

        // Update he_ad: now in face2, prev is he_bc, next is he_dc
        self.half_edges[he_ad.0 as usize].face = Some(face2);
        self.half_edges[he_ad.0 as usize].prev = he_bc;
        self.half_edges[he_ad.0 as usize].next = he_ba; // he_dc

        // Update he_db: now in face1, prev is he_cd, next is he_ca
        self.half_edges[he_db.0 as usize].face = Some(face1);
        self.half_edges[he_db.0 as usize].prev = he_ab; // he_cd
        self.half_edges[he_db.0 as usize].next = he_ca;

        // ===== PHASE 4: UPDATE FACES =====
        self.faces[face1.0 as usize].half_edge = he_ab; // he_cd
        self.faces[face2.0 as usize].half_edge = he_ba; // he_dc

        // ===== PHASE 5: UPDATE EDGE MAP =====
        self.edge_map.remove(&(v_a, v_b));
        self.edge_map.remove(&(v_b, v_a));
        self.edge_map.insert((v_c, v_d), he_ab);
        self.edge_map.insert((v_d, v_c), he_ba);

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
            self.vertices[v_c.0 as usize].outgoing_half_edge = Some(he_ab); // he_cd
        }
        if self.vertices[v_d.0 as usize]
            .outgoing_half_edge
            .map_or(true, |e| {
                self.half_edge(e)
                    .map_or(true, |h| h.origin != v_d || h.face.is_none())
            })
        {
            self.vertices[v_d.0 as usize].outgoing_half_edge = Some(he_ba); // he_dc
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
            "flip_edge_topology: flipped edge {:?} from ({:?}->{:?}) to ({:?}->{:?})",
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

        // Collect all vertices from orphaned half-edges that need outgoing edge updates
        // This includes v0, v2 (third vertex of face ABC), and v3 (third vertex of face ABD)
        // but NOT v1 which is being removed
        let mut vertices_to_fix: Vec<VertexId> = vec![v0_id];
        for &he_id in &orphaned_half_edges {
            let origin = self.half_edges[he_id.0 as usize].origin;
            if origin != v1_id && !vertices_to_fix.contains(&origin) {
                vertices_to_fix.push(origin);
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

        // Redirect half-edges from v1 to v0 (only non-orphaned ones)
        for he in self.half_edges.iter_mut() {
            if he.origin == v1_id && he.face.is_some() {
                he.origin = v0_id;
            }
        }

        // Update edge map for redirected edges
        let keys_to_update: Vec<_> = self
            .edge_map
            .iter()
            .filter(|((from, _), _)| *from == v1_id)
            .map(|(k, v)| (*k, *v))
            .collect();
        for ((from, to), he_id) in keys_to_update {
            self.edge_map.remove(&(from, to));
            // Only keep non-orphaned edges in the map
            if self.half_edges[he_id.0 as usize].face.is_some() {
                self.edge_map.insert((v0_id, to), he_id);
            }
        }

        // Move v0 to midpoint
        self.vertices[v0_id.0 as usize].position = mid_pos;
        self.vertices[v0_id.0 as usize].normal = mid_normal;

        // Fix outgoing_half_edge for ALL affected vertices (v0, v2, v3, etc.)
        // This is critical: if any vertex's outgoing edge points to an orphaned half-edge,
        // mesh traversal will fail
        for vid in vertices_to_fix {
            self.vertices[vid.0 as usize].outgoing_half_edge = self
                .half_edges
                .iter()
                .enumerate()
                .find(|(_, he)| he.origin == vid && he.face.is_some())
                .map(|(i, _)| HalfEdgeId(i as u32));
        }

        // Mark v1 as having no outgoing edge (effectively removed)
        self.vertices[v1_id.0 as usize].outgoing_half_edge = None;

        trace!(
            "collapse_edge_topology: END removed {} faces, {} orphaned half-edges",
            removed_faces.len(),
            orphaned_half_edges.len()
        );
        Some(removed_faces)
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

        let (new_vertex, new_faces) = result.unwrap();

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
}
