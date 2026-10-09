//! Canonical authoritative chunk data; derived bounds/grid are rebuilt.
use super::*;
use painting::half_edge::HalfEdgeDocument;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkedDocument {
    next_chunk_id: u32,
    next_original_vertex_id: u32,
    config: [usize; 3],
    chunks: Vec<ChunkDocument>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkDocument {
    id: u32,
    mesh: HalfEdgeDocument,
    local_to_original: Vec<(u32, u32)>,
    original_to_local: Vec<(u32, u32)>,
    boundaries: Vec<(u32, Vec<(u32, u32, u32)>)>,
}
impl ChunkedDocument {
    pub fn record_count(&self) -> usize {
        self.chunks.iter().fold(0usize, |n, c| {
            n.saturating_add(c.mesh.record_count())
                .saturating_add(c.local_to_original.len())
                .saturating_add(c.original_to_local.len())
                .saturating_add(c.boundaries.iter().map(|(_, v)| v.len() + 1).sum::<usize>())
        })
    }
    pub fn restore(self) -> Result<ChunkedMesh, String> {
        if self.chunks.is_empty()
            || self.chunks.len() > 256
            || self.record_count() > 1_000_000
            || self.next_chunk_id == u32::MAX
            || self.next_original_vertex_id == u32::MAX
            || self.config[0] == 0
            || self.config[0] > self.config[2]
            || self.config[2] > self.config[1]
            || self.config[1] > 1_000_000
        {
            return Err("Invalid or over-limit project chunks/configuration".into());
        }
        let mut out = ChunkedMesh::with_config(ChunkConfig {
            min_faces: self.config[0],
            max_faces: self.config[1],
            target_faces: self.config[2],
        });
        out.next_chunk_id = self.next_chunk_id;
        out.next_original_vertex_id = self.next_original_vertex_id;
        for c in self.chunks {
            let nlocal = c.local_to_original.len();
            let norig = c.original_to_local.len();
            let nb = c.boundaries.len();
            let local_to_original = c
                .local_to_original
                .into_iter()
                .map(|(a, b)| (VertexId(a), VertexId(b)))
                .collect::<HashMap<_, _>>();
            let original_to_local = c
                .original_to_local
                .into_iter()
                .map(|(a, b)| (VertexId(a), VertexId(b)))
                .collect::<HashMap<_, _>>();
            let boundary_vertices = c
                .boundaries
                .into_iter()
                .map(|(a, b)| {
                    (
                        VertexId(a),
                        b.into_iter()
                            .map(|(c, v, o)| {
                                BoundaryVertex::new(ChunkId(c), VertexId(v), VertexId(o))
                            })
                            .collect(),
                    )
                })
                .collect::<HashMap<_, _>>();
            if nlocal != local_to_original.len()
                || norig != original_to_local.len()
                || nb != boundary_vertices.len()
            {
                return Err("Duplicate project chunk map keys".into());
            }
            let mut chunk = MeshChunk {
                id: ChunkId(c.id),
                bounds: Aabb::empty(),
                mesh: c.mesh.restore()?,
                local_to_original,
                original_to_local,
                boundary_vertices,
                dirty: true,
                topology_changed: true,
            };
            chunk.recalculate_bounds();
            if out.chunks.insert(chunk.id, chunk).is_some() {
                return Err("Duplicate project chunk IDs".into());
            }
        }
        // Bound derived seam reconstruction as well as serialized references.
        let mut copies: HashMap<VertexId, usize> = HashMap::new();
        for c in out.chunks.values() {
            for original in c.local_to_original.values() {
                *copies.entry(*original).or_default() += 1;
            }
        }
        let expected_refs = copies.values().fold(0usize, |n, &count| {
            n.saturating_add(count.saturating_mul(count.saturating_sub(1)))
        });
        if expected_refs > 1_000_000 {
            return Err("Project derived seam relationship budget exceeded".into());
        }
        // The pipeline owns the authoritative geometric safety admission API.
        let mut pipeline = crate::SculptingPipeline::new(crate::BrushPreset::default());
        pipeline
            .reset_history(0, &mut out, None)
            .map_err(|e| format!("Project geometry rejected: {e}"))?;
        Ok(out)
    }
}
impl ChunkedMesh {
    pub fn document(&self) -> ChunkedDocument {
        let mut chunks: Vec<_> = self
            .chunks
            .values()
            .map(|c| {
                let mut local_to_original: Vec<_> = c
                    .local_to_original
                    .iter()
                    .map(|(a, b)| (a.0, b.0))
                    .collect();
                local_to_original.sort_unstable();
                let mut original_to_local: Vec<_> = c
                    .original_to_local
                    .iter()
                    .map(|(a, b)| (a.0, b.0))
                    .collect();
                original_to_local.sort_unstable();
                let mut boundaries: Vec<_> = c
                    .boundary_vertices
                    .iter()
                    .map(|(a, b)| {
                        (
                            a.0,
                            b.iter()
                                .map(|v| (v.chunk_id.0, v.vertex_id.0, v.original_vertex_id.0))
                                .collect(),
                        )
                    })
                    .collect();
                boundaries.sort_by_key(|v| v.0);
                ChunkDocument {
                    id: c.id.0,
                    mesh: c.mesh.document(),
                    local_to_original,
                    original_to_local,
                    boundaries,
                }
            })
            .collect();
        chunks.sort_by_key(|c| c.id);
        ChunkedDocument {
            next_chunk_id: self.next_chunk_id,
            next_original_vertex_id: self.next_original_vertex_id,
            config: [
                self.config.min_faces,
                self.config.max_faces,
                self.config.target_faces,
            ],
            chunks,
        }
    }
}
