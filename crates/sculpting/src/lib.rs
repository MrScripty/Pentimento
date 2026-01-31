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
//! - **Pipeline**: Orchestrates stroke → deform → tessellate → GPU sync

pub mod types;

pub use types::{
    ChunkConfig, DeformationType, SculptDab, SculptStrokeHeader, SculptStrokePacket,
    TessellationAction, TessellationConfig,
};
