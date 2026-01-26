//! Mesh surface storage for 3D mesh painting
//!
//! This module provides two storage modes for painting on 3D meshes:
//!
//! - **MeshUvSurface**: For meshes with UV coordinates. Paints to a texture atlas
//!   that can be applied to the mesh's material. This is the preferred mode when
//!   UVs are available as it's more memory-efficient.
//!
//! - **MeshPtexSurface**: For meshes without UVs. Uses per-face textures (Ptex-style)
//!   where each triangle gets its own small texture tile. Requires adjacency data
//!   for proper edge blending.

use std::collections::HashMap;

use crate::tiles::TiledSurface;
use crate::types::BlendMode;

/// Surface storage for a paintable mesh using UV texture atlas.
///
/// This is the preferred storage mode when the mesh has proper UV coordinates.
/// The atlas texture can be directly applied to the mesh's material.
pub struct MeshUvSurface {
    /// UV atlas texture (reuses existing TiledSurface infrastructure)
    pub atlas: TiledSurface,
    /// Padding in pixels at UV seams for bleed prevention
    pub seam_padding: u32,
    /// Mesh ID this surface belongs to
    pub mesh_id: u32,
}

impl MeshUvSurface {
    /// Create a new UV-based mesh surface with the given resolution.
    ///
    /// # Arguments
    /// * `mesh_id` - Unique identifier for the mesh
    /// * `width` - Texture atlas width in pixels
    /// * `height` - Texture atlas height in pixels
    /// * `seam_padding` - Pixels of padding at UV seams (default: 2)
    pub fn new(mesh_id: u32, width: u32, height: u32, seam_padding: u32) -> Self {
        Self {
            atlas: TiledSurface::with_default_tile_size(width, height),
            seam_padding,
            mesh_id,
        }
    }

    /// Apply a dab at UV coordinates.
    ///
    /// # Arguments
    /// * `uv` - UV coordinates (0-1 range)
    /// * `radius` - Brush radius in texture pixels
    /// * `color` - RGBA color
    /// * `opacity` - Opacity (0-1)
    /// * `hardness` - Edge hardness (0-1)
    /// * `blend_mode` - How to blend with existing pixels
    /// * `angle` - Brush rotation in radians
    /// * `aspect_ratio` - Brush aspect ratio (1.0 = circular)
    ///
    /// # Returns
    /// Bounding box of affected region, or None if outside surface.
    pub fn apply_dab(
        &mut self,
        uv: glam::Vec2,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
        angle: f32,
        aspect_ratio: f32,
    ) -> Option<(u32, u32, u32, u32)> {
        // Convert UV to pixel coordinates
        let width = self.atlas.surface().width as f32;
        let height = self.atlas.surface().height as f32;

        // UV origin is typically top-left with V inverted for textures
        let x = uv.x * width;
        let y = (1.0 - uv.y) * height; // Flip Y for texture coordinates

        self.atlas
            .apply_dab_ellipse(x, y, radius, color, opacity, hardness, blend_mode, angle, aspect_ratio)
    }

    /// Get the underlying tiled surface for GPU upload.
    pub fn surface(&self) -> &TiledSurface {
        &self.atlas
    }

    /// Get mutable access to the underlying tiled surface.
    pub fn surface_mut(&mut self) -> &mut TiledSurface {
        &mut self.atlas
    }

    /// Check if there are dirty tiles to upload.
    pub fn has_dirty_tiles(&self) -> bool {
        self.atlas.has_dirty_tiles()
    }

    /// Get texture dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.atlas.surface().width, self.atlas.surface().height)
    }
}

/// Per-face texture data for Ptex-style storage.
#[derive(Debug, Clone)]
pub struct PtexFace {
    /// Face (triangle) index in the mesh
    pub face_id: u32,
    /// Pixel data for this face's texture (RGBA f32)
    pub pixels: Vec<[f32; 4]>,
    /// Resolution of this face's texture (square: resolution x resolution)
    pub resolution: u32,
    /// Whether this face has been modified and needs GPU upload
    pub dirty: bool,
}

impl PtexFace {
    /// Create a new Ptex face with the given resolution.
    pub fn new(face_id: u32, resolution: u32) -> Self {
        let pixel_count = (resolution * resolution) as usize;
        Self {
            face_id,
            pixels: vec![[0.0, 0.0, 0.0, 0.0]; pixel_count],
            resolution,
            dirty: false,
        }
    }

    /// Get a pixel at local coordinates.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        if x >= self.resolution || y >= self.resolution {
            return None;
        }
        let idx = (y * self.resolution + x) as usize;
        self.pixels.get(idx).copied()
    }

    /// Set a pixel at local coordinates.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [f32; 4]) {
        if x >= self.resolution || y >= self.resolution {
            return;
        }
        let idx = (y * self.resolution + x) as usize;
        if idx < self.pixels.len() {
            self.pixels[idx] = color;
            self.dirty = true;
        }
    }

    /// Blend a pixel with existing color.
    pub fn blend_pixel(&mut self, x: u32, y: u32, color: [f32; 4], opacity: f32) {
        if let Some(existing) = self.get_pixel(x, y) {
            let blended = [
                existing[0] * (1.0 - opacity) + color[0] * opacity,
                existing[1] * (1.0 - opacity) + color[1] * opacity,
                existing[2] * (1.0 - opacity) + color[2] * opacity,
                existing[3] * (1.0 - opacity) + color[3] * opacity,
            ];
            self.set_pixel(x, y, blended);
        }
    }

    /// Clear the face to transparent.
    pub fn clear(&mut self) {
        for pixel in &mut self.pixels {
            *pixel = [0.0, 0.0, 0.0, 0.0];
        }
        self.dirty = true;
    }
}

/// Edge adjacency information for Ptex blending.
///
/// For each face, stores which neighboring faces share each edge.
/// Edge indices: 0 = v0-v1, 1 = v1-v2, 2 = v2-v0
#[derive(Debug, Clone, Default)]
pub struct FaceAdjacency {
    /// For each face_id, a list of (edge_index, neighbor_face_id, neighbor_edge_index)
    pub edges: HashMap<u32, Vec<(u8, u32, u8)>>,
}

impl FaceAdjacency {
    /// Create empty adjacency data.
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// Add an adjacency relationship.
    ///
    /// # Arguments
    /// * `face_a` - First face ID
    /// * `edge_a` - Edge index on first face (0-2)
    /// * `face_b` - Second face ID (neighbor)
    /// * `edge_b` - Edge index on neighbor face (0-2)
    pub fn add_edge(&mut self, face_a: u32, edge_a: u8, face_b: u32, edge_b: u8) {
        self.edges
            .entry(face_a)
            .or_default()
            .push((edge_a, face_b, edge_b));
    }

    /// Get neighbors for a face.
    pub fn get_neighbors(&self, face_id: u32) -> Option<&Vec<(u8, u32, u8)>> {
        self.edges.get(&face_id)
    }
}

/// Surface storage for meshes without UVs using per-face textures (Ptex-style).
///
/// Each triangle face gets its own small texture tile. Painting is done in
/// face-local coordinates derived from barycentric coordinates.
pub struct MeshPtexSurface {
    /// Per-face textures indexed by face_id
    pub faces: HashMap<u32, PtexFace>,
    /// Default resolution for new face textures
    pub default_resolution: u32,
    /// Adjacency data for edge blending
    pub adjacency: FaceAdjacency,
    /// Mesh ID this surface belongs to
    pub mesh_id: u32,
}

impl MeshPtexSurface {
    /// Create a new Ptex-style mesh surface.
    ///
    /// # Arguments
    /// * `mesh_id` - Unique identifier for the mesh
    /// * `default_resolution` - Default resolution for face textures (e.g., 16 or 32)
    pub fn new(mesh_id: u32, default_resolution: u32) -> Self {
        Self {
            faces: HashMap::new(),
            default_resolution,
            adjacency: FaceAdjacency::new(),
            mesh_id,
        }
    }

    /// Get or create a face texture.
    pub fn get_or_create_face(&mut self, face_id: u32) -> &mut PtexFace {
        let resolution = self.default_resolution;
        self.faces
            .entry(face_id)
            .or_insert_with(|| PtexFace::new(face_id, resolution))
    }

    /// Apply a dab at face-local coordinates.
    ///
    /// # Arguments
    /// * `face_id` - Face to paint on
    /// * `local_coords` - Coordinates within the face (0 to resolution)
    /// * `radius` - Brush radius in face-texture pixels
    /// * `color` - RGBA color
    /// * `opacity` - Opacity (0-1)
    /// * `hardness` - Edge hardness (0-1)
    /// * `blend_mode` - How to blend with existing pixels
    ///
    /// # Returns
    /// Whether any pixels were modified.
    pub fn apply_dab(
        &mut self,
        face_id: u32,
        local_coords: glam::Vec2,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
    ) -> bool {
        let face = self.get_or_create_face(face_id);
        let _resolution = face.resolution as f32;

        // Dab center in pixel coordinates
        let cx = local_coords.x;
        let cy = local_coords.y;

        // Bounding box
        let x_min = ((cx - radius).floor().max(0.0) as u32).min(face.resolution);
        let y_min = ((cy - radius).floor().max(0.0) as u32).min(face.resolution);
        let x_max = ((cx + radius).ceil().max(0.0) as u32).min(face.resolution);
        let y_max = ((cy + radius).ceil().max(0.0) as u32).min(face.resolution);

        if x_min >= x_max || y_min >= y_max {
            return false;
        }

        let radius_sq = radius * radius;
        let mut modified = false;

        for py in y_min..y_max {
            for px in x_min..x_max {
                let dx = (px as f32 + 0.5) - cx;
                let dy = (py as f32 + 0.5) - cy;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq > radius_sq {
                    continue;
                }

                let distance_normalized = (dist_sq.sqrt() / radius).min(1.0);
                let falloff = calculate_hardness_falloff(distance_normalized, hardness);

                if falloff > 0.0 {
                    let effective_opacity = opacity * falloff;
                    match blend_mode {
                        BlendMode::Normal => {
                            face.blend_pixel(px, py, color, effective_opacity);
                        }
                        BlendMode::Erase => {
                            if let Some(existing) = face.get_pixel(px, py) {
                                let new_alpha = existing[3] * (1.0 - effective_opacity);
                                face.set_pixel(
                                    px,
                                    py,
                                    [existing[0], existing[1], existing[2], new_alpha],
                                );
                            }
                        }
                    }
                    modified = true;
                }
            }
        }

        modified
    }

    /// Set adjacency data for edge blending.
    pub fn set_adjacency(&mut self, adjacency: FaceAdjacency) {
        self.adjacency = adjacency;
    }

    /// Get all dirty faces that need GPU upload.
    pub fn get_dirty_faces(&self) -> Vec<u32> {
        self.faces
            .iter()
            .filter(|(_, face)| face.dirty)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Clear dirty flags on all faces.
    pub fn clear_dirty_flags(&mut self) {
        for face in self.faces.values_mut() {
            face.dirty = false;
        }
    }

    /// Get face count.
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }
}

/// Calculate falloff based on hardness (same as in tiles.rs).
#[inline]
fn calculate_hardness_falloff(distance_normalized: f32, hardness: f32) -> f32 {
    if hardness >= 1.0 {
        if distance_normalized <= 1.0 {
            1.0
        } else {
            0.0
        }
    } else {
        let t = distance_normalized.clamp(0.0, 1.0);
        let soft = 1.0 - t;
        let hard = if t <= 1.0 { 1.0 } else { 0.0 };
        soft * (1.0 - hardness) + hard * hardness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_uv_surface_creation() {
        let surface = MeshUvSurface::new(1, 512, 512, 2);
        assert_eq!(surface.mesh_id, 1);
        assert_eq!(surface.dimensions(), (512, 512));
        assert_eq!(surface.seam_padding, 2);
    }

    #[test]
    fn test_mesh_uv_surface_apply_dab() {
        let mut surface = MeshUvSurface::new(1, 256, 256, 2);

        // Apply dab at center
        let result = surface.apply_dab(
            glam::Vec2::new(0.5, 0.5),
            10.0,
            [1.0, 0.0, 0.0, 1.0],
            1.0,
            1.0,
            BlendMode::Normal,
            0.0,
            1.0,
        );

        assert!(result.is_some());
        assert!(surface.has_dirty_tiles());
    }

    #[test]
    fn test_ptex_face_creation() {
        let face = PtexFace::new(0, 16);
        assert_eq!(face.face_id, 0);
        assert_eq!(face.resolution, 16);
        assert_eq!(face.pixels.len(), 16 * 16);
    }

    #[test]
    fn test_ptex_face_get_set_pixel() {
        let mut face = PtexFace::new(0, 16);

        face.set_pixel(5, 5, [1.0, 0.0, 0.0, 1.0]);
        let pixel = face.get_pixel(5, 5);

        assert!(pixel.is_some());
        assert_eq!(pixel.unwrap(), [1.0, 0.0, 0.0, 1.0]);
        assert!(face.dirty);
    }

    #[test]
    fn test_ptex_face_out_of_bounds() {
        let face = PtexFace::new(0, 16);

        assert!(face.get_pixel(20, 20).is_none());
    }

    #[test]
    fn test_mesh_ptex_surface_apply_dab() {
        let mut surface = MeshPtexSurface::new(1, 32);

        let modified = surface.apply_dab(
            0,
            glam::Vec2::new(16.0, 16.0),
            5.0,
            [1.0, 0.0, 0.0, 1.0],
            1.0,
            1.0,
            BlendMode::Normal,
        );

        assert!(modified);
        assert!(surface.faces.contains_key(&0));
        assert_eq!(surface.get_dirty_faces().len(), 1);
    }

    #[test]
    fn test_face_adjacency() {
        let mut adj = FaceAdjacency::new();

        // Face 0 edge 1 connects to face 1 edge 0
        adj.add_edge(0, 1, 1, 0);

        let neighbors = adj.get_neighbors(0);
        assert!(neighbors.is_some());
        assert_eq!(neighbors.unwrap().len(), 1);
        assert_eq!(neighbors.unwrap()[0], (1, 1, 0));
    }
}
