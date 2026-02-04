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
pub mod budget;
pub mod chunking;
pub mod deformation;
pub mod gpu;
pub mod pipeline;
pub mod spatial;
pub mod tessellation;
pub mod types;

pub use brush::{BrushInput, BrushPreset, DabResult, FalloffCurve, SculptBrushEngine, StrokeState};
pub use chunking::{
    get_original_vertex_id, is_boundary_vertex, merge_chunks, merge_two_chunks, partition_mesh,
    rebalance_chunks, split_chunk, sync_vertex_position, Aabb, BoundaryVertex, ChunkId,
    ChunkedMesh, MergeResult, MeshChunk, PartitionConfig,
};
pub use deformation::{
    apply_autosmooth, apply_crease, apply_deformation, apply_flatten, apply_grab, apply_inflate,
    apply_pinch, apply_pull, apply_push, apply_smooth, DabInfo, DeformationContext,
    DeformationResult,
};
pub use gpu::{
    recalculate_face_normals_for_dirty, recalculate_normals_for_dirty,
    update_normals_after_deformation, DirtyVertices, SyncResult,
};
#[cfg(feature = "bevy")]
pub use gpu::{create_chunk_meshes, remove_chunk_meshes, sync_chunk_to_gpu, sync_chunks_to_gpu};
pub use spatial::{OctreeConfig, VertexOctree};
pub use tessellation::{
    calculate_collapse_position, calculate_edge_screen_length, calculate_split_position,
    calculate_world_edge_length, can_collapse_edge, can_split_edge, collapse_edge,
    dihedral_angle, evaluate_edge, evaluate_edge_curvature, interpolate_vertex_attributes,
    split_edge, tessellate_at_brush, tessellate_at_brush_budget, would_cause_flip,
    CollapseResult, CurvatureEvaluation,
    EdgeEvaluation, ScreenSpaceConfig, SplitResult, TessellationDecision, TessellationStats,
};
pub use budget::VertexBudget;
pub use types::{
    ChunkConfig, DeformationType, SculptDab, SculptStrokeHeader, SculptStrokePacket,
    TessellationAction, TessellationConfig, TessellationMode,
};
pub use pipeline::{
    DabProcessResult, PipelineConfig, SculptingPipeline, StrokeEndResult,
};
