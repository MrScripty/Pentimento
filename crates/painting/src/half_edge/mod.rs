//! Half-edge mesh data structure for mesh editing operations
//!
//! Provides topology information (vertex-face adjacency, edge loops, etc.)
//! that is not available in a simple triangle soup representation.

mod construction;
mod modification;
mod topology;
mod types;
mod validation;

use std::collections::HashMap;

pub use types::{Face, FaceId, HalfEdge, HalfEdgeError, HalfEdgeId, Vertex, VertexId};
pub use validation::ManifoldError;

use types::HalfEdge as HalfEdgeInternal;
use types::Vertex as VertexInternal;
use types::Face as FaceInternal;

/// Half-edge mesh data structure
///
/// Provides efficient topology queries for mesh editing operations.
#[derive(Debug, Clone)]
pub struct HalfEdgeMesh {
    pub(crate) vertices: Vec<VertexInternal>,
    pub(crate) half_edges: Vec<HalfEdgeInternal>,
    pub(crate) faces: Vec<FaceInternal>,
    /// Map from (origin, destination) vertex pair to half-edge
    pub(crate) edge_map: HashMap<(VertexId, VertexId), HalfEdgeId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::prelude::*;

    fn create_test_triangle_mesh() -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.5, 1.0, 0.0],
            ],
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vec![
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        );
        mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
        mesh
    }

    #[test]
    fn test_from_bevy_mesh_single_triangle() {
        let mesh = create_test_triangle_mesh();
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        assert_eq!(he_mesh.vertex_count(), 3);
        assert_eq!(he_mesh.face_count(), 1);
        assert!(he_mesh.validate().is_ok());
    }

    #[test]
    fn test_face_vertices() {
        let mesh = create_test_triangle_mesh();
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        let verts = he_mesh.get_face_vertices(FaceId(0));
        assert_eq!(verts.len(), 3);
    }

    #[test]
    fn test_vertex_faces() {
        let mesh = create_test_triangle_mesh();
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        let faces = he_mesh.get_vertex_faces(VertexId(0));
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0], FaceId(0));
    }
}
