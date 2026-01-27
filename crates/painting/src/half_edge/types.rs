//! Type definitions for the half-edge mesh data structure.

use bevy::prelude::*;

/// Type-safe vertex identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexId(pub u32);

/// Type-safe half-edge identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HalfEdgeId(pub u32);

/// Type-safe face identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceId(pub u32);

/// A vertex in the half-edge mesh
#[derive(Debug, Clone)]
pub struct Vertex {
    pub id: VertexId,
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Option<Vec2>,
    /// One outgoing half-edge from this vertex (arbitrary choice if multiple)
    pub outgoing_half_edge: Option<HalfEdgeId>,
    /// Original index in the source Bevy mesh (for attribute mapping)
    pub source_index: u32,
}

/// A half-edge in the mesh
///
/// Each edge in the mesh is represented by two half-edges pointing in opposite
/// directions. Half-edges store connectivity information for traversing the mesh.
#[derive(Debug, Clone)]
pub struct HalfEdge {
    pub id: HalfEdgeId,
    /// The vertex this half-edge originates from
    pub origin: VertexId,
    /// The opposite half-edge (None for boundary edges)
    pub twin: Option<HalfEdgeId>,
    /// The next half-edge around the face (counter-clockwise)
    pub next: HalfEdgeId,
    /// The previous half-edge around the face (counter-clockwise)
    pub prev: HalfEdgeId,
    /// The face this half-edge borders (None for boundary half-edges)
    pub face: Option<FaceId>,
}

/// A face (polygon) in the mesh
#[derive(Debug, Clone)]
pub struct Face {
    pub id: FaceId,
    /// One half-edge on the boundary of this face
    pub half_edge: HalfEdgeId,
    /// Cached face normal
    pub normal: Vec3,
}

/// Errors that can occur during half-edge mesh operations
#[derive(Debug, thiserror::Error)]
pub enum HalfEdgeError {
    #[error("Mesh has no position attribute")]
    NoPositions,
    #[error("Mesh has no indices")]
    NoIndices,
    #[error("Invalid mesh topology: {0}")]
    InvalidTopology(String),
    #[error("Non-manifold edge detected")]
    NonManifoldEdge,
}
