//! Spatial data structures for efficient sculpting queries.
//!
//! This module provides octree-based spatial indexing for:
//! - Fast vertex queries within brush radius
//! - Efficient chunk lookup by position
//! - Collision detection during deformation

use glam::Vec3;
use painting::half_edge::VertexId;

/// Configuration for octree construction.
#[derive(Debug, Clone)]
pub struct OctreeConfig {
    /// Maximum depth of the octree.
    pub max_depth: u32,
    /// Maximum items per leaf node before splitting.
    pub max_items_per_leaf: usize,
    /// Minimum node size (prevents infinite subdivision).
    pub min_node_size: f32,
}

impl Default for OctreeConfig {
    fn default() -> Self {
        Self {
            max_depth: 8,
            max_items_per_leaf: 16,
            min_node_size: 0.01,
        }
    }
}

/// A spatial octree for efficient vertex queries.
///
/// Used during sculpting to quickly find all vertices within brush radius.
#[derive(Debug)]
pub struct VertexOctree {
    root: OctreeNode,
    config: OctreeConfig,
}

/// An item stored in the octree: vertex ID and position.
#[derive(Debug, Clone, Copy)]
struct OctreeItem {
    vertex_id: VertexId,
    position: Vec3,
}

/// A node in the octree (either internal or leaf).
#[derive(Debug)]
enum OctreeNode {
    /// Leaf node containing items.
    Leaf {
        bounds: Aabb,
        items: Vec<OctreeItem>,
    },
    /// Internal node with 8 children.
    Internal {
        bounds: Aabb,
        children: Box<[Option<OctreeNode>; 8]>,
    },
}

/// Axis-aligned bounding box (duplicated here to avoid circular dependency).
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::MAX),
            max: Vec3::splat(f32::MIN),
        }
    }

    pub fn include_point(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        let closest = center.clamp(self.min, self.max);
        closest.distance_squared(center) <= radius * radius
    }

    /// Get the octant index for a point (0-7).
    fn octant_for_point(&self, point: Vec3) -> usize {
        let center = self.center();
        let mut index = 0;
        if point.x >= center.x {
            index |= 1;
        }
        if point.y >= center.y {
            index |= 2;
        }
        if point.z >= center.z {
            index |= 4;
        }
        index
    }

    /// Get the bounds for a specific octant.
    fn octant_bounds(&self, octant: usize) -> Aabb {
        let center = self.center();
        let min = Vec3::new(
            if octant & 1 != 0 { center.x } else { self.min.x },
            if octant & 2 != 0 { center.y } else { self.min.y },
            if octant & 4 != 0 { center.z } else { self.min.z },
        );
        let max = Vec3::new(
            if octant & 1 != 0 { self.max.x } else { center.x },
            if octant & 2 != 0 { self.max.y } else { center.y },
            if octant & 4 != 0 { self.max.z } else { center.z },
        );
        Aabb::new(min, max)
    }
}

impl VertexOctree {
    /// Create a new empty octree with the given bounds.
    pub fn new(bounds: Aabb) -> Self {
        Self::with_config(bounds, OctreeConfig::default())
    }

    /// Create a new octree with custom configuration.
    pub fn with_config(bounds: Aabb, config: OctreeConfig) -> Self {
        Self {
            root: OctreeNode::Leaf {
                bounds,
                items: Vec::new(),
            },
            config,
        }
    }

    /// Build an octree from vertex positions.
    pub fn from_vertices<'a>(
        vertices: impl Iterator<Item = (VertexId, Vec3)>,
    ) -> Self {
        // First pass: compute bounds
        let mut bounds = Aabb::empty();
        let items: Vec<OctreeItem> = vertices
            .map(|(id, pos)| {
                bounds.include_point(pos);
                OctreeItem {
                    vertex_id: id,
                    position: pos,
                }
            })
            .collect();

        // Expand bounds slightly to handle edge cases
        let padding = bounds.size() * 0.01 + Vec3::splat(0.001);
        bounds.min -= padding;
        bounds.max += padding;

        let mut octree = Self::new(bounds);

        // Insert all vertices
        for item in items {
            octree.insert(item.vertex_id, item.position);
        }

        octree
    }

    /// Insert a vertex into the octree.
    pub fn insert(&mut self, vertex_id: VertexId, position: Vec3) {
        let config = self.config.clone();
        Self::insert_into_node(&mut self.root, vertex_id, position, 0, &config);
    }

    fn insert_into_node(
        node: &mut OctreeNode,
        vertex_id: VertexId,
        position: Vec3,
        depth: u32,
        config: &OctreeConfig,
    ) {
        match node {
            OctreeNode::Leaf { bounds, items } => {
                items.push(OctreeItem { vertex_id, position });

                // Check if we need to split
                if items.len() > config.max_items_per_leaf
                    && depth < config.max_depth
                    && bounds.size().min_element() > config.min_node_size * 2.0
                {
                    // Convert to internal node
                    let old_items = std::mem::take(items);
                    let old_bounds = *bounds;

                    *node = OctreeNode::Internal {
                        bounds: old_bounds,
                        children: Box::new([None, None, None, None, None, None, None, None]),
                    };

                    // Re-insert all items
                    for item in old_items {
                        Self::insert_into_node(node, item.vertex_id, item.position, depth, config);
                    }
                }
            }
            OctreeNode::Internal { bounds, children } => {
                let octant = bounds.octant_for_point(position);

                if children[octant].is_none() {
                    let child_bounds = bounds.octant_bounds(octant);
                    children[octant] = Some(OctreeNode::Leaf {
                        bounds: child_bounds,
                        items: Vec::new(),
                    });
                }

                if let Some(child) = &mut children[octant] {
                    Self::insert_into_node(child, vertex_id, position, depth + 1, config);
                }
            }
        }
    }

    /// Query all vertices within a sphere.
    pub fn query_sphere(&self, center: Vec3, radius: f32) -> Vec<VertexId> {
        let mut results = Vec::new();
        Self::query_sphere_node(&self.root, center, radius, &mut results);
        results
    }

    fn query_sphere_node(
        node: &OctreeNode,
        center: Vec3,
        radius: f32,
        results: &mut Vec<VertexId>,
    ) {
        match node {
            OctreeNode::Leaf { bounds, items } => {
                if !bounds.intersects_sphere(center, radius) {
                    return;
                }

                let radius_sq = radius * radius;
                for item in items {
                    if item.position.distance_squared(center) <= radius_sq {
                        results.push(item.vertex_id);
                    }
                }
            }
            OctreeNode::Internal { bounds, children } => {
                if !bounds.intersects_sphere(center, radius) {
                    return;
                }

                for child in children.iter().flatten() {
                    Self::query_sphere_node(child, center, radius, results);
                }
            }
        }
    }

    /// Query all vertices within an axis-aligned bounding box.
    pub fn query_aabb(&self, query_bounds: &Aabb) -> Vec<VertexId> {
        let mut results = Vec::new();
        Self::query_aabb_node(&self.root, query_bounds, &mut results);
        results
    }

    fn query_aabb_node(
        node: &OctreeNode,
        query_bounds: &Aabb,
        results: &mut Vec<VertexId>,
    ) {
        match node {
            OctreeNode::Leaf { bounds, items } => {
                if !Self::aabb_intersects(bounds, query_bounds) {
                    return;
                }

                for item in items {
                    if query_bounds.contains_point(item.position) {
                        results.push(item.vertex_id);
                    }
                }
            }
            OctreeNode::Internal { bounds, children } => {
                if !Self::aabb_intersects(bounds, query_bounds) {
                    return;
                }

                for child in children.iter().flatten() {
                    Self::query_aabb_node(child, query_bounds, results);
                }
            }
        }
    }

    fn aabb_intersects(a: &Aabb, b: &Aabb) -> bool {
        a.min.x <= b.max.x
            && a.max.x >= b.min.x
            && a.min.y <= b.max.y
            && a.max.y >= b.min.y
            && a.min.z <= b.max.z
            && a.max.z >= b.min.z
    }

    /// Get the total number of items in the octree.
    pub fn len(&self) -> usize {
        Self::count_items(&self.root)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn count_items(node: &OctreeNode) -> usize {
        match node {
            OctreeNode::Leaf { items, .. } => items.len(),
            OctreeNode::Internal { children, .. } => {
                children.iter().flatten().map(Self::count_items).sum()
            }
        }
    }

    /// Clear all items from the octree.
    pub fn clear(&mut self) {
        let bounds = match &self.root {
            OctreeNode::Leaf { bounds, .. } => *bounds,
            OctreeNode::Internal { bounds, .. } => *bounds,
        };
        self.root = OctreeNode::Leaf {
            bounds,
            items: Vec::new(),
        };
    }

    /// Update a vertex position in the octree.
    ///
    /// This is a simple remove + insert operation.
    pub fn update(&mut self, vertex_id: VertexId, old_position: Vec3, new_position: Vec3) {
        self.remove(vertex_id, old_position);
        self.insert(vertex_id, new_position);
    }

    /// Remove a vertex from the octree.
    pub fn remove(&mut self, vertex_id: VertexId, position: Vec3) -> bool {
        Self::remove_from_node(&mut self.root, vertex_id, position)
    }

    fn remove_from_node(node: &mut OctreeNode, vertex_id: VertexId, position: Vec3) -> bool {
        match node {
            OctreeNode::Leaf { bounds, items } => {
                if !bounds.contains_point(position) {
                    return false;
                }

                if let Some(idx) = items.iter().position(|item| item.vertex_id == vertex_id) {
                    items.swap_remove(idx);
                    return true;
                }
                false
            }
            OctreeNode::Internal { bounds, children } => {
                if !bounds.contains_point(position) {
                    return false;
                }

                let octant = bounds.octant_for_point(position);
                if let Some(child) = &mut children[octant] {
                    Self::remove_from_node(child, vertex_id, position)
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_octree_insert_and_query() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::splat(10.0));
        let mut octree = VertexOctree::new(bounds);

        // Insert some vertices
        octree.insert(VertexId(0), Vec3::new(1.0, 1.0, 1.0));
        octree.insert(VertexId(1), Vec3::new(2.0, 2.0, 2.0));
        octree.insert(VertexId(2), Vec3::new(8.0, 8.0, 8.0));

        assert_eq!(octree.len(), 3);

        // Query sphere around (1.5, 1.5, 1.5)
        let results = octree.query_sphere(Vec3::new(1.5, 1.5, 1.5), 1.5);
        assert!(results.contains(&VertexId(0)));
        assert!(results.contains(&VertexId(1)));
        assert!(!results.contains(&VertexId(2)));
    }

    #[test]
    fn test_octree_from_vertices() {
        let vertices = [
            (VertexId(0), Vec3::new(0.0, 0.0, 0.0)),
            (VertexId(1), Vec3::new(1.0, 0.0, 0.0)),
            (VertexId(2), Vec3::new(0.0, 1.0, 0.0)),
            (VertexId(3), Vec3::new(1.0, 1.0, 1.0)),
        ];

        let octree = VertexOctree::from_vertices(vertices.into_iter());
        assert_eq!(octree.len(), 4);

        // Query all vertices
        let results = octree.query_sphere(Vec3::splat(0.5), 2.0);
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_octree_remove() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::splat(10.0));
        let mut octree = VertexOctree::new(bounds);

        octree.insert(VertexId(0), Vec3::new(1.0, 1.0, 1.0));
        octree.insert(VertexId(1), Vec3::new(2.0, 2.0, 2.0));

        assert_eq!(octree.len(), 2);

        octree.remove(VertexId(0), Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(octree.len(), 1);

        let results = octree.query_sphere(Vec3::new(1.5, 1.5, 1.5), 2.0);
        assert!(!results.contains(&VertexId(0)));
        assert!(results.contains(&VertexId(1)));
    }
}
