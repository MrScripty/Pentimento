//! Modification methods for HalfEdgeMesh.

use bevy::prelude::*;

use super::types::{FaceId, VertexId};
use super::HalfEdgeMesh;

impl HalfEdgeMesh {
    /// Set the position of a vertex
    pub fn set_vertex_position(&mut self, vertex_id: VertexId, position: Vec3) {
        if let Some(v) = self.vertex_mut(vertex_id) {
            v.position = position;
        }
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
}
