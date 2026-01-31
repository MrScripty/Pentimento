//! 3D Sculpting system for Pentimento.
//!
//! This crate provides dynamic tessellation sculpting with:
//! - Brush-based deformation (Push, Pull, Smooth, etc.)
//! - Screen-space adaptive tessellation (like Blender DynTopo)
//! - Mesh chunking for optimized GPU updates
//! - Dab-based stroke recording for P2P sync and undo/redo
//!
//! # Architecture
//!
//! The sculpting system follows the same dab-based approach as the painting
//! system, enabling future P2P synchronization and deterministic undo/redo
//! via stroke replay.
//!
//! ## Key Components
//!
//! - **Types**: Core data structures for dabs, strokes, and configuration
//! - **Brush**: Dab generation from input events
//! - **Deformation**: Vertex displacement algorithms
//! - **Tessellation**: Edge split/collapse for constant mesh density
//! - **Chunking**: Spatial partitioning for localized GPU updates
//! - **Spatial**: Octree for efficient brush-to-vertex queries
//! - **Pipeline**: Orchestrates stroke → deform → tessellate → GPU sync

pub mod brush;
pub mod chunking;
pub mod deformation;
pub mod gpu;
pub mod spatial;
pub mod tessellation;
pub mod types;

pub use brush::{BrushInput, BrushPreset, DabResult, FalloffCurve, SculptBrushEngine, StrokeState};
pub use chunking::{
    Aabb, BoundaryVertex, ChunkId, ChunkedMesh, MergeResult, MeshChunk, PartitionConfig,
};
pub use deformation::{
    apply_crease, apply_deformation, apply_flatten, apply_grab, apply_inflate, apply_pinch,
    apply_pull, apply_push, apply_smooth, DabInfo, DeformationContext, DeformationResult,
};
pub use gpu::{
    recalculate_face_normals_for_dirty, recalculate_normals_for_dirty,
    update_normals_after_deformation, DirtyVertices, SyncResult,
};
#[cfg(feature = "bevy")]
pub use gpu::{create_chunk_meshes, remove_chunk_meshes, sync_chunk_to_gpu, sync_chunks_to_gpu};
pub use spatial::{OctreeConfig, VertexOctree};
pub use tessellation::{
    can_collapse_edge, collapse_edge, evaluate_edge, split_edge, tessellate_at_brush,
    CollapseResult, EdgeEvaluation, ScreenSpaceConfig, SplitResult, TessellationDecision,
    TessellationStats,
};
pub use types::{
    ChunkConfig, DeformationType, SculptDab, SculptStrokeHeader, SculptStrokePacket,
    TessellationAction, TessellationConfig,
};
