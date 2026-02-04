//! Vertex deformation algorithms for sculpting.
//!
//! This module provides the core deformation functions that modify vertex
//! positions based on brush input. Each deformation type has different
//! behavior and may require different context (e.g., neighboring vertices
//! for smoothing).

use glam::Vec3;
use painting::half_edge::{HalfEdgeMesh, VertexId};
use std::collections::HashMap;

use crate::brush::{DabResult, FalloffCurve};
use crate::spatial::VertexOctree;
use crate::types::DeformationType;

/// Context for applying deformations.
///
/// Contains all the data needed to apply a dab to a mesh.
pub struct DeformationContext<'a> {
    /// The mesh being deformed
    pub mesh: &'a mut HalfEdgeMesh,
    /// Spatial index for fast vertex queries
    pub octree: &'a VertexOctree,
    /// Falloff curve for this brush
    pub falloff: FalloffCurve,
    /// Direction of stroke movement (for grab/crease)
    pub stroke_direction: Option<Vec3>,
    /// Previous dab position (for grab)
    pub previous_position: Option<Vec3>,
}

/// Result of applying a dab.
#[derive(Debug, Default)]
pub struct DeformationResult {
    /// Vertices that were modified
    pub modified_vertices: Vec<VertexId>,
    /// Original positions before modification (for undo)
    pub original_positions: HashMap<VertexId, Vec3>,
}

/// Apply a dab to the mesh, modifying vertex positions.
///
/// Returns the list of modified vertices and their original positions.
pub fn apply_dab(ctx: &mut DeformationContext<'_>, dab: &DabResult) -> DeformationResult {
    // Query vertices within brush radius
    let affected_vertices = ctx.octree.query_sphere(dab.position, dab.radius);

    if affected_vertices.is_empty() {
        return DeformationResult::default();
    }

    // Store original positions for undo
    let mut original_positions = HashMap::new();
    for &vertex_id in &affected_vertices {
        if let Some(vertex) = ctx.mesh.vertex(vertex_id) {
            original_positions.insert(vertex_id, vertex.position);
        }
    }

    // The actual deformation is dispatched by type through apply_deformation()
    // This function just prepares the context and returns affected vertices

    DeformationResult {
        modified_vertices: affected_vertices,
        original_positions,
    }
}

/// Information about a dab for deformation functions.
#[derive(Debug, Clone, Copy)]
pub struct DabInfo {
    pub position: Vec3,
    pub normal: Vec3,
    pub radius: f32,
    pub strength: f32,
}

/// Apply push deformation - moves vertices along surface normal.
pub fn apply_push(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
) -> HashMap<VertexId, Vec3> {
    let mut original_positions = HashMap::new();

    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist) * dab.strength;

        // Use vertex normal for direction
        let direction = vertex.normal.normalize_or_zero();
        let displacement = direction * strength * 0.1; // Scale factor for reasonable movement

        original_positions.insert(vertex_id, vertex.position);
        let new_pos = vertex.position + displacement;
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply pull deformation - moves vertices toward brush center.
pub fn apply_pull(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
) -> HashMap<VertexId, Vec3> {
    let mut original_positions = HashMap::new();

    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist) * dab.strength;

        // Direction toward brush center
        let direction = (dab.position - vertex.position).normalize_or_zero();
        let displacement = direction * strength * 0.1;

        original_positions.insert(vertex_id, vertex.position);
        let new_pos = vertex.position + displacement;
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply grab deformation - moves vertices along stroke direction.
pub fn apply_grab(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
    stroke_delta: Vec3,
) -> HashMap<VertexId, Vec3> {
    let mut original_positions = HashMap::new();

    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist);

        // Move along stroke direction
        let displacement = stroke_delta * strength;

        original_positions.insert(vertex_id, vertex.position);
        let new_pos = vertex.position + displacement;
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply smooth deformation - averages vertex positions with neighbors.
pub fn apply_smooth(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
) -> HashMap<VertexId, Vec3> {
    // First pass: calculate target positions without modifying
    let mut target_positions: HashMap<VertexId, Vec3> = HashMap::new();
    let mut original_positions = HashMap::new();

    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        // Get neighboring vertices
        let neighbors = mesh.get_adjacent_vertices(vertex_id);
        if neighbors.is_empty() {
            continue;
        }

        // Calculate average position of neighbors
        let mut avg_pos = Vec3::ZERO;
        let mut count = 0;
        for neighbor_id in neighbors {
            if let Some(neighbor) = mesh.vertex(neighbor_id) {
                avg_pos += neighbor.position;
                count += 1;
            }
        }

        if count == 0 {
            continue;
        }

        avg_pos /= count as f32;

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist) * dab.strength;

        // Blend toward average
        let current_pos = vertex.position;
        let new_pos = current_pos.lerp(avg_pos, strength);

        original_positions.insert(vertex_id, current_pos);
        target_positions.insert(vertex_id, new_pos);
    }

    // Second pass: apply all modifications
    for (vertex_id, new_pos) in target_positions {
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply tangent-plane-projected Laplacian smooth to dampen dab ripples.
///
/// For each affected vertex: compute neighbor average, project the offset
/// onto the tangent plane (strip normal component), blend with configurable
/// strength. Two-pass: compute all targets first, then apply.
pub fn apply_autosmooth(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
    autosmooth_strength: f32,
) {
    if autosmooth_strength <= 0.0 || vertices.is_empty() {
        return;
    }

    // Pass 1: compute target positions without modifying mesh
    let mut targets: Vec<(VertexId, Vec3)> = Vec::new();

    for &vid in vertices {
        let Some(vertex) = mesh.vertex(vid) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let neighbors = mesh.get_adjacent_vertices(vid);
        if neighbors.is_empty() {
            continue;
        }

        // Compute average neighbor position (Laplacian target)
        let mut avg = Vec3::ZERO;
        let mut count = 0;
        for nid in neighbors {
            if let Some(n) = mesh.vertex(nid) {
                avg += n.position;
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        avg /= count as f32;

        // Laplacian offset projected onto tangent plane
        let offset = avg - vertex.position;
        let normal = vertex.normal.normalize_or_zero();
        let tangent_offset = offset - normal * offset.dot(normal);

        // Scale by falloff and autosmooth strength
        let normalized_dist = distance / dab.radius;
        let effective = falloff.evaluate(normalized_dist) * autosmooth_strength;

        targets.push((vid, vertex.position + tangent_offset * effective));
    }

    // Pass 2: apply all modifications
    for (vid, pos) in targets {
        mesh.set_vertex_position(vid, pos);
    }
}

/// Apply flatten deformation - moves vertices toward average plane.
pub fn apply_flatten(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
) -> HashMap<VertexId, Vec3> {
    let mut original_positions = HashMap::new();

    if vertices.is_empty() {
        return original_positions;
    }

    // Calculate average plane from affected vertices
    let mut avg_pos = Vec3::ZERO;
    let mut avg_normal = Vec3::ZERO;
    let mut count = 0;

    for &vertex_id in vertices {
        if let Some(vertex) = mesh.vertex(vertex_id) {
            let distance = vertex.position.distance(dab.position);
            if distance <= dab.radius {
                avg_pos += vertex.position;
                avg_normal += vertex.normal;
                count += 1;
            }
        }
    }

    if count == 0 {
        return original_positions;
    }

    avg_pos /= count as f32;
    let plane_normal = avg_normal.normalize_or_zero();

    // Use dab normal as plane normal if no average normal
    let plane_normal = if plane_normal.length_squared() > 0.01 {
        plane_normal
    } else {
        dab.normal
    };

    // Project vertices onto the plane
    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist) * dab.strength;

        // Project onto plane
        let to_vertex = vertex.position - avg_pos;
        let dist_to_plane = to_vertex.dot(plane_normal);
        let projected = vertex.position - plane_normal * dist_to_plane;

        // Blend toward projected position
        let new_pos = vertex.position.lerp(projected, strength);

        original_positions.insert(vertex_id, vertex.position);
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply inflate deformation - moves vertices along their own normals.
pub fn apply_inflate(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
) -> HashMap<VertexId, Vec3> {
    let mut original_positions = HashMap::new();

    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist) * dab.strength;

        // Move along vertex's own normal
        let direction = vertex.normal.normalize_or_zero();
        let displacement = direction * strength * 0.1;

        original_positions.insert(vertex_id, vertex.position);
        let new_pos = vertex.position + displacement;
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply pinch deformation - moves vertices toward brush center (XZ plane).
pub fn apply_pinch(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
) -> HashMap<VertexId, Vec3> {
    let mut original_positions = HashMap::new();

    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist) * dab.strength;

        // Direction toward brush center, but projected onto tangent plane
        let to_center = dab.position - vertex.position;

        // Project onto tangent plane (remove normal component)
        let normal = vertex.normal.normalize_or_zero();
        let tangent_dir = to_center - normal * to_center.dot(normal);
        let tangent_dir = tangent_dir.normalize_or_zero();

        let displacement = tangent_dir * strength * 0.1;

        original_positions.insert(vertex_id, vertex.position);
        let new_pos = vertex.position + displacement;
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply crease deformation - creates a groove along stroke path.
pub fn apply_crease(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    falloff: FalloffCurve,
    stroke_direction: Vec3,
) -> HashMap<VertexId, Vec3> {
    let mut original_positions = HashMap::new();

    let stroke_dir = stroke_direction.normalize_or_zero();
    if stroke_dir.length_squared() < 0.01 {
        return original_positions;
    }

    for &vertex_id in vertices {
        let Some(vertex) = mesh.vertex(vertex_id) else {
            continue;
        };

        let distance = vertex.position.distance(dab.position);
        if distance > dab.radius {
            continue;
        }

        let normalized_dist = distance / dab.radius;
        let strength = falloff.evaluate(normalized_dist) * dab.strength;

        // Calculate perpendicular distance from stroke line
        let to_vertex = vertex.position - dab.position;
        let along_stroke = to_vertex.dot(stroke_dir);
        let perpendicular = to_vertex - stroke_dir * along_stroke;
        let perp_dist = perpendicular.length();

        // Pinch toward the stroke line and push down
        let pinch_strength = if perp_dist > 0.001 {
            let perp_normalized = perp_dist / (dab.radius * 0.5);
            (1.0 - perp_normalized.min(1.0)) * strength
        } else {
            strength
        };

        // Move toward stroke line (pinch) and down (crease)
        let pinch_dir = -perpendicular.normalize_or_zero();
        let down_dir = -dab.normal;

        let displacement = pinch_dir * pinch_strength * 0.05 + down_dir * strength * 0.05;

        original_positions.insert(vertex_id, vertex.position);
        let new_pos = vertex.position + displacement;
        mesh.set_vertex_position(vertex_id, new_pos);
    }

    original_positions
}

/// Apply deformation based on type.
pub fn apply_deformation(
    mesh: &mut HalfEdgeMesh,
    vertices: &[VertexId],
    dab: &DabInfo,
    deformation_type: DeformationType,
    falloff: FalloffCurve,
    stroke_direction: Option<Vec3>,
    stroke_delta: Option<Vec3>,
) -> HashMap<VertexId, Vec3> {
    match deformation_type {
        DeformationType::Push => apply_push(mesh, vertices, dab, falloff),
        DeformationType::Pull => apply_pull(mesh, vertices, dab, falloff),
        DeformationType::Grab => {
            let delta = stroke_delta.unwrap_or(Vec3::ZERO);
            apply_grab(mesh, vertices, dab, falloff, delta)
        }
        DeformationType::Smooth => apply_smooth(mesh, vertices, dab, falloff),
        DeformationType::Flatten => apply_flatten(mesh, vertices, dab, falloff),
        DeformationType::Inflate => apply_inflate(mesh, vertices, dab, falloff),
        DeformationType::Pinch => apply_pinch(mesh, vertices, dab, falloff),
        DeformationType::Crease => {
            let direction = stroke_direction.unwrap_or(Vec3::X);
            apply_crease(mesh, vertices, dab, falloff, direction)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_falloff_affects_displacement() {
        // Linear falloff at distance 0.5 (half radius) should give 0.5 strength
        let falloff_value = FalloffCurve::Linear.evaluate(0.5);
        assert!((falloff_value - 0.5).abs() < 0.001);

        // Smooth falloff at 0.5 should be different (S-curve)
        let smooth_value = FalloffCurve::Smooth.evaluate(0.5);
        assert!(smooth_value > 0.4 && smooth_value < 0.6);
    }

    #[test]
    fn test_dab_info() {
        let dab = DabInfo {
            position: Vec3::new(0.5, 0.0, 0.0),
            normal: Vec3::Y,
            radius: 1.0,
            strength: 0.5,
        };

        assert_eq!(dab.position, Vec3::new(0.5, 0.0, 0.0));
        assert_eq!(dab.radius, 1.0);
    }
}
