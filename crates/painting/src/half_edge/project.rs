//! Lossless document representation. Connectivity is checked before traversal.
use super::*;
use bevy::math::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HalfEdgeDocument {
    vertices: Vec<VertexDocument>,
    edges: Vec<EdgeDocument>,
    faces: Vec<FaceDocument>,
    edge_map: Vec<(u32, u32, u32)>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VertexDocument {
    id: u32,
    position: [f32; 3],
    normal: [f32; 3],
    uv: Option<[f32; 2]>,
    outgoing: Option<u32>,
    source_index: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeDocument {
    id: u32,
    origin: u32,
    uv: Option<[f32; 2]>,
    twin: Option<u32>,
    next: u32,
    prev: u32,
    face: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FaceDocument {
    id: u32,
    edge: u32,
    normal: [f32; 3],
}

impl HalfEdgeDocument {
    pub fn record_count(&self) -> usize {
        self.vertices
            .len()
            .saturating_add(self.edges.len())
            .saturating_add(self.faces.len())
            .saturating_add(self.edge_map.len())
    }
    pub fn restore(self) -> Result<HalfEdgeMesh, String> {
        if self.record_count() > 1_000_000 {
            return Err("Project topology record limit exceeded".into());
        }
        let finite = |a: &[f32]| a.iter().all(|v| v.is_finite());
        let nv = self.vertices.len();
        let ne = self.edges.len();
        let nf = self.faces.len();
        if self.vertices.iter().enumerate().any(|(i, v)| {
            v.id as usize != i
                || !finite(&v.position)
                || !finite(&v.normal)
                || v.uv.is_some_and(|uv| !finite(&uv))
                || v.outgoing.is_some_and(|e| e as usize >= ne)
        }) || self.edges.iter().enumerate().any(|(i, e)| {
            e.id as usize != i
                || e.origin as usize >= nv
                || e.next as usize >= ne
                || e.prev as usize >= ne
                || e.twin.is_some_and(|t| t as usize >= ne)
                || e.face.is_some_and(|f| f as usize >= nf)
                || e.uv.is_some_and(|uv| !finite(&uv))
        }) || self
            .faces
            .iter()
            .enumerate()
            .any(|(i, f)| f.id as usize != i || f.edge as usize >= ne || !finite(&f.normal))
            || self
                .edge_map
                .iter()
                .any(|&(a, b, e)| a as usize >= nv || b as usize >= nv || e as usize >= ne)
        {
            return Err("Invalid project topology references or numbers".into());
        }
        let map_len = self.edge_map.len();
        let mesh = HalfEdgeMesh {
            vertices: self
                .vertices
                .into_iter()
                .map(|v| Vertex {
                    id: VertexId(v.id),
                    position: Vec3::from_array(v.position),
                    normal: Vec3::from_array(v.normal),
                    uv: v.uv.map(Vec2::from_array),
                    outgoing_half_edge: v.outgoing.map(HalfEdgeId),
                    source_index: v.source_index,
                })
                .collect(),
            half_edges: self
                .edges
                .into_iter()
                .map(|e| HalfEdge {
                    id: HalfEdgeId(e.id),
                    origin: VertexId(e.origin),
                    corner_uv: e.uv.map(Vec2::from_array),
                    twin: e.twin.map(HalfEdgeId),
                    next: HalfEdgeId(e.next),
                    prev: HalfEdgeId(e.prev),
                    face: e.face.map(FaceId),
                })
                .collect(),
            faces: self
                .faces
                .into_iter()
                .map(|f| Face {
                    id: FaceId(f.id),
                    half_edge: HalfEdgeId(f.edge),
                    normal: Vec3::from_array(f.normal),
                })
                .collect(),
            edge_map: self
                .edge_map
                .into_iter()
                .map(|(a, b, e)| ((VertexId(a), VertexId(b)), HalfEdgeId(e)))
                .collect(),
        };
        if mesh.edge_map.len() != map_len {
            return Err("Duplicate project topology map keys".into());
        }
        mesh.validate().map_err(|e| e.to_string())?;
        Ok(mesh)
    }
}
impl HalfEdgeMesh {
    pub fn document(&self) -> HalfEdgeDocument {
        let mut edge_map: Vec<_> = self
            .edge_map
            .iter()
            .map(|((a, b), e)| (a.0, b.0, e.0))
            .collect();
        edge_map.sort_unstable();
        HalfEdgeDocument {
            vertices: self
                .vertices
                .iter()
                .map(|v| VertexDocument {
                    id: v.id.0,
                    position: v.position.to_array(),
                    normal: v.normal.to_array(),
                    uv: v.uv.map(|v| v.to_array()),
                    outgoing: v.outgoing_half_edge.map(|v| v.0),
                    source_index: v.source_index,
                })
                .collect(),
            edges: self
                .half_edges
                .iter()
                .map(|e| EdgeDocument {
                    id: e.id.0,
                    origin: e.origin.0,
                    uv: e.corner_uv.map(|v| v.to_array()),
                    twin: e.twin.map(|v| v.0),
                    next: e.next.0,
                    prev: e.prev.0,
                    face: e.face.map(|v| v.0),
                })
                .collect(),
            faces: self
                .faces
                .iter()
                .map(|f| FaceDocument {
                    id: f.id.0,
                    edge: f.half_edge.0,
                    normal: f.normal.to_array(),
                })
                .collect(),
            edge_map,
        }
    }
}
