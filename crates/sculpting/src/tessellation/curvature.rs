//! Curvature-based edge evaluation for budget tessellation.
//!
//! Provides dihedral angle computation and curvature-prioritized split/collapse
//! decisions. Used by the `BudgetCurvature` tessellation mode where edges are
//! split at high-curvature areas first and collapsed at low-curvature areas first,
//! subject to a global vertex budget.

use crate::tessellation::TessellationDecision;
use crate::types::TessellationConfig;
use painting::half_edge::{HalfEdgeId, HalfEdgeMesh};

/// Result of evaluating an edge for curvature-based tessellation.
#[derive(Debug, Clone, Copy)]
pub struct CurvatureEvaluation {
    /// Dihedral angle in radians (0 = flat, PI = sharp crease)
    pub dihedral_angle: f32,
    /// Decision based on curvature thresholds
    pub decision: TessellationDecision,
}

/// Calculate the dihedral angle at an edge.
///
/// The dihedral angle is the angle between the normals of the two faces
/// adjacent to an edge. It measures local surface curvature:
/// - 0 radians = perfectly flat (coplanar faces)
/// - PI radians = maximally sharp crease (faces folded back on each other)
///
/// Returns `None` if the edge is a boundary edge (no twin) or if face
/// data is unavailable.
pub fn dihedral_angle(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> Option<f32> {
    let he = mesh.half_edge(edge_id)?;
    let face_a_id = he.face?;
    let twin_id = he.twin?;
    let twin = mesh.half_edge(twin_id)?;
    let face_b_id = twin.face?;

    let face_a = mesh.face(face_a_id)?;
    let face_b = mesh.face(face_b_id)?;

    let normal_a = face_a.normal;
    let normal_b = face_b.normal;

    // Skip if either normal is zero (degenerate face)
    if normal_a.length_squared() < 1e-10 || normal_b.length_squared() < 1e-10 {
        return None;
    }

    let dot = normal_a.dot(normal_b).clamp(-1.0, 1.0);
    Some(dot.acos())
}

/// Evaluate an edge for curvature-based tessellation.
///
/// Compares the dihedral angle against the configured curvature thresholds.
/// Boundary edges (no twin) are treated as maximum curvature (always split candidates).
pub fn evaluate_edge_curvature(
    mesh: &HalfEdgeMesh,
    edge_id: HalfEdgeId,
    config: &TessellationConfig,
) -> CurvatureEvaluation {
    // Boundary edges: treat as high curvature (split candidates)
    let he = match mesh.half_edge(edge_id) {
        Some(he) => he,
        None => {
            return CurvatureEvaluation {
                dihedral_angle: 0.0,
                decision: TessellationDecision::None,
            };
        }
    };

    let angle = if he.twin.is_none() {
        // Boundary edge — treat as max curvature
        std::f32::consts::PI
    } else {
        dihedral_angle(mesh, edge_id).unwrap_or(0.0)
    };

    let decision = if angle > config.curvature_split_threshold {
        TessellationDecision::Split
    } else if angle < config.curvature_collapse_threshold {
        TessellationDecision::Collapse
    } else {
        TessellationDecision::None
    };

    CurvatureEvaluation {
        dihedral_angle: angle,
        decision,
    }
}

#[cfg(all(test, feature = "bevy"))]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
    use painting::half_edge::HalfEdgeMesh;

    /// Build a simple quad (two triangles sharing an edge) for testing.
    /// Layout:
    ///   v2---v3
    ///   | \ |
    ///   v0---v1
    /// Face 0: v0, v1, v2 (counter-clockwise)
    /// Face 1: v1, v3, v2 (counter-clockwise)
    fn build_flat_quad() -> HalfEdgeMesh {
        let positions = vec![
            [0.0, 0.0, 0.0], // v0
            [1.0, 0.0, 0.0], // v1
            [0.0, 1.0, 0.0], // v2
            [1.0, 1.0, 0.0], // v3
        ];
        let normals = vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let indices = vec![0u32, 1, 2, 1, 3, 2];

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_indices(Indices::U32(indices));

        let mut he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();
        he_mesh.recalculate_face_normals();
        he_mesh
    }

    /// Build a quad where one triangle is rotated to create a known dihedral angle.
    fn build_angled_quad(angle_degrees: f32) -> HalfEdgeMesh {
        let angle_rad = angle_degrees.to_radians();
        let positions = vec![
            [0.0, 0.0, 0.0],                                       // v0
            [1.0, 0.0, 0.0],                                       // v1
            [0.0, 1.0, 0.0],                                       // v2
            [1.0, angle_rad.cos(), angle_rad.sin()],                // v3 rotated around X axis
        ];
        let normals = vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let indices = vec![0u32, 1, 2, 1, 3, 2];

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_indices(Indices::U32(indices));

        let mut he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();
        he_mesh.recalculate_face_normals();
        he_mesh
    }

    #[test]
    fn test_flat_quad_dihedral_angle_is_zero() {
        let mesh = build_flat_quad();

        // Find the shared edge (v1 -> v2 or v2 -> v1)
        let shared_edge = mesh.find_half_edge(
            painting::half_edge::VertexId(1),
            painting::half_edge::VertexId(2),
        );

        if let Some(edge_id) = shared_edge {
            let angle = dihedral_angle(&mesh, edge_id);
            if let Some(a) = angle {
                assert!(
                    a.abs() < 0.01,
                    "Flat quad should have ~0 dihedral angle, got {}",
                    a
                );
            }
        }
    }

    #[test]
    fn test_angled_quad_dihedral_angle() {
        let mesh = build_angled_quad(90.0);

        // Find the shared edge
        let shared_edge = mesh.find_half_edge(
            painting::half_edge::VertexId(1),
            painting::half_edge::VertexId(2),
        );

        if let Some(edge_id) = shared_edge {
            let angle = dihedral_angle(&mesh, edge_id);
            if let Some(a) = angle {
                // Should be approximately 90 degrees = PI/2
                let expected = std::f32::consts::FRAC_PI_2;
                assert!(
                    (a - expected).abs() < 0.2,
                    "90-degree fold should have ~PI/2 dihedral angle, got {} (expected {})",
                    a,
                    expected
                );
            }
        }
    }

    #[test]
    fn test_evaluate_edge_curvature_split() {
        let mesh = build_angled_quad(90.0);
        let config = TessellationConfig::default(); // curvature_split_threshold = 0.1 rad

        let shared_edge = mesh.find_half_edge(
            painting::half_edge::VertexId(1),
            painting::half_edge::VertexId(2),
        );

        if let Some(edge_id) = shared_edge {
            let eval = evaluate_edge_curvature(&mesh, edge_id, &config);
            // 90 degrees > 0.1 radians threshold → should want to split
            assert_eq!(
                eval.decision,
                TessellationDecision::Split,
                "High curvature edge should be marked for split"
            );
        }
    }

    #[test]
    fn test_evaluate_edge_curvature_flat() {
        let mesh = build_flat_quad();
        let config = TessellationConfig::default();

        let shared_edge = mesh.find_half_edge(
            painting::half_edge::VertexId(1),
            painting::half_edge::VertexId(2),
        );

        if let Some(edge_id) = shared_edge {
            let eval = evaluate_edge_curvature(&mesh, edge_id, &config);
            // ~0 degrees < 0.03 radians threshold → should want to collapse
            assert_eq!(
                eval.decision,
                TessellationDecision::Collapse,
                "Flat edge should be marked for collapse"
            );
        }
    }
}
