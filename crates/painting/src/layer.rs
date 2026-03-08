//! Layer system for multi-layer painting
//!
//! Provides a traditional layer stack where each layer has its own surface,
//! visibility, and opacity. Layers are composited bottom-to-top into a
//! flattened output surface for GPU upload.

use crate::constants::DEFAULT_TILE_SIZE;
use crate::tiles::TiledSurface;

/// A single painting layer
pub struct Layer {
    /// Unique layer ID
    pub id: u32,
    /// Human-readable name
    pub name: String,
    /// Whether this layer is visible during compositing
    pub visible: bool,
    /// Layer opacity (0.0-1.0), applied during compositing
    pub opacity: f32,
    /// The pixel data for this layer
    pub surface: TiledSurface,
}

impl Layer {
    /// Create a new layer with the given dimensions
    pub fn new(id: u32, name: String, width: u32, height: u32) -> Self {
        Self {
            id,
            name,
            visible: true,
            opacity: 1.0,
            surface: TiledSurface::with_default_tile_size(width, height),
        }
    }
}

/// Manages an ordered stack of layers and composites them
pub struct LayerStack {
    /// Ordered layers, index 0 = bottom
    layers: Vec<Layer>,
    /// ID of the currently active (painting target) layer
    active_layer_id: u32,
    /// Next ID to assign
    next_id: u32,
    /// Composited output surface (same dimensions as layers)
    composite: TiledSurface,
    /// Surface dimensions
    width: u32,
    height: u32,
}

impl LayerStack {
    /// Create a new layer stack with one default "Background" layer
    pub fn new(width: u32, height: u32) -> Self {
        let background = Layer::new(0, "Background".to_string(), width, height);
        Self {
            layers: vec![background],
            active_layer_id: 0,
            next_id: 1,
            composite: TiledSurface::with_default_tile_size(width, height),
            width,
            height,
        }
    }

    /// Get the active layer (the one being painted on)
    pub fn active_layer(&self) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == self.active_layer_id)
    }

    /// Get mutable reference to the active layer
    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        let id = self.active_layer_id;
        self.layers.iter_mut().find(|l| l.id == id)
    }

    /// Get the active layer ID
    pub fn active_layer_id(&self) -> u32 {
        self.active_layer_id
    }

    /// Get a layer by ID
    pub fn layer(&self, layer_id: u32) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == layer_id)
    }

    /// Get a mutable layer by ID
    pub fn layer_mut(&mut self, layer_id: u32) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == layer_id)
    }

    /// Add a new layer above the active layer, returns its id
    pub fn add_layer(&mut self, name: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let layer_name = if name.is_empty() {
            format!("Layer {}", id)
        } else {
            name
        };

        let layer = Layer::new(id, layer_name, self.width, self.height);

        // Insert above the active layer
        let active_idx = self
            .layers
            .iter()
            .position(|l| l.id == self.active_layer_id);
        let insert_idx = match active_idx {
            Some(idx) => idx + 1,
            None => self.layers.len(),
        };

        self.layers.insert(insert_idx, layer);
        self.active_layer_id = id;
        id
    }

    /// Remove a layer by id (cannot remove the last layer)
    pub fn remove_layer(&mut self, layer_id: u32) -> bool {
        if self.layers.len() <= 1 {
            return false;
        }

        let Some(idx) = self.layers.iter().position(|l| l.id == layer_id) else {
            return false;
        };

        self.layers.remove(idx);

        // If we removed the active layer, select an adjacent one
        if self.active_layer_id == layer_id {
            let new_idx = idx.min(self.layers.len() - 1);
            self.active_layer_id = self.layers[new_idx].id;
        }

        true
    }

    /// Set which layer is active
    pub fn set_active(&mut self, layer_id: u32) -> bool {
        if self.layers.iter().any(|l| l.id == layer_id) {
            self.active_layer_id = layer_id;
            true
        } else {
            false
        }
    }

    /// Set layer visibility
    pub fn set_visibility(&mut self, layer_id: u32, visible: bool) {
        if let Some(layer) = self.layer_mut(layer_id) {
            layer.visible = visible;
        }
    }

    /// Set layer opacity
    pub fn set_opacity(&mut self, layer_id: u32, opacity: f32) {
        if let Some(layer) = self.layer_mut(layer_id) {
            layer.opacity = opacity.clamp(0.0, 1.0);
        }
    }

    /// Reorder: move layer to a new index position
    pub fn reorder(&mut self, layer_id: u32, new_index: usize) {
        let Some(old_idx) = self.layers.iter().position(|l| l.id == layer_id) else {
            return;
        };

        let new_index = new_index.min(self.layers.len() - 1);
        if old_idx == new_index {
            return;
        }

        let layer = self.layers.remove(old_idx);
        self.layers.insert(new_index, layer);
    }

    /// Rename a layer
    pub fn rename(&mut self, layer_id: u32, name: String) {
        if let Some(layer) = self.layer_mut(layer_id) {
            layer.name = name;
        }
    }

    /// Composite all visible layers bottom-to-top into the composite surface.
    ///
    /// This flattens the layer stack for GPU upload. All composite tiles
    /// are marked dirty so the GPU texture gets updated.
    pub fn composite(&mut self) {
        let width = self.width;
        let height = self.height;
        let tile_size = DEFAULT_TILE_SIZE;
        let tiles_x = (width + tile_size - 1) / tile_size;
        let tiles_y = (height + tile_size - 1) / tile_size;

        // Clear composite to transparent
        self.composite.surface_mut().clear([0.0, 0.0, 0.0, 0.0]);

        // Blend each visible layer bottom-to-top
        for layer in &self.layers {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }

            let src_pixels = layer.surface.surface().pixels();
            let dst_pixels = self.composite.surface_mut().pixels_mut();
            let layer_opacity = layer.opacity;

            for (dst, src) in dst_pixels.iter_mut().zip(src_pixels.iter()) {
                let src_a = src[3] * layer_opacity;
                if src_a <= 0.0 {
                    continue;
                }

                let inv_src_a = 1.0 - src_a;
                *dst = [
                    src[0] * src_a + dst[0] * inv_src_a,
                    src[1] * src_a + dst[1] * inv_src_a,
                    src[2] * src_a + dst[2] * inv_src_a,
                    src_a + dst[3] * inv_src_a,
                ];
            }
        }

        // Mark all composite tiles as dirty
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                self.composite.mark_dirty(tx * tile_size, ty * tile_size);
            }
        }
    }

    /// Get the composited output surface (after calling composite())
    pub fn composited_surface(&self) -> &TiledSurface {
        &self.composite
    }

    /// Get mutable composited output surface
    pub fn composited_surface_mut(&mut self) -> &mut TiledSurface {
        &mut self.composite
    }

    /// Get layer metadata for UI sync
    pub fn layer_info(&self) -> Vec<LayerInfo> {
        self.layers
            .iter()
            .map(|l| LayerInfo {
                id: l.id,
                name: l.name.clone(),
                visible: l.visible,
                opacity: l.opacity,
                is_active: l.id == self.active_layer_id,
            })
            .collect()
    }

    /// Clear dirty tile flags on all individual layers.
    /// Called after compositing and extracting dirty tiles.
    pub fn clear_layer_dirty_flags(&mut self) {
        for layer in &mut self.layers {
            layer.surface.take_dirty_tiles();
        }
    }

    /// Get the number of layers
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Check if the stack is empty (should never be)
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

/// Layer metadata for UI synchronization (mirroring pentimento_ipc::LayerInfo)
#[derive(Debug, Clone)]
pub struct LayerInfo {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub is_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_stack_creation() {
        let stack = LayerStack::new(256, 256);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.active_layer_id(), 0);
        assert_eq!(stack.active_layer().unwrap().name, "Background");
    }

    #[test]
    fn test_add_layer() {
        let mut stack = LayerStack::new(256, 256);
        let id = stack.add_layer("Layer 1".to_string());
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.active_layer_id(), id);
    }

    #[test]
    fn test_remove_layer() {
        let mut stack = LayerStack::new(256, 256);
        let id = stack.add_layer("Layer 1".to_string());
        assert!(stack.remove_layer(id));
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_cannot_remove_last_layer() {
        let mut stack = LayerStack::new(256, 256);
        assert!(!stack.remove_layer(0));
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_set_active() {
        let mut stack = LayerStack::new(256, 256);
        let id = stack.add_layer("Layer 1".to_string());
        assert!(stack.set_active(0));
        assert_eq!(stack.active_layer_id(), 0);
        assert!(stack.set_active(id));
        assert_eq!(stack.active_layer_id(), id);
    }

    #[test]
    fn test_visibility() {
        let mut stack = LayerStack::new(256, 256);
        stack.set_visibility(0, false);
        assert!(!stack.layer(0).unwrap().visible);
    }

    #[test]
    fn test_composite() {
        let mut stack = LayerStack::new(4, 4);

        // Paint red on background
        stack
            .active_layer_mut()
            .unwrap()
            .surface
            .surface_mut()
            .clear([1.0, 0.0, 0.0, 1.0]);

        // Add layer and paint blue
        let id = stack.add_layer("Blue".to_string());
        stack
            .active_layer_mut()
            .unwrap()
            .surface
            .surface_mut()
            .clear([0.0, 0.0, 1.0, 0.5]);

        stack.composite();

        // Composite should show blue blended over red
        let pixel = stack
            .composited_surface()
            .surface()
            .get_pixel(0, 0)
            .unwrap();
        // Blue at 0.5 alpha over red at 1.0 alpha:
        // r = 0.0 * 0.5 + 1.0 * 0.5 = 0.5
        // b = 1.0 * 0.5 + 0.0 * 0.5 = 0.5
        assert!((pixel[0] - 0.5).abs() < 0.01);
        assert!((pixel[2] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_layer_info() {
        let mut stack = LayerStack::new(256, 256);
        stack.add_layer("Layer 1".to_string());
        let info = stack.layer_info();
        assert_eq!(info.len(), 2);
        assert_eq!(info[0].name, "Background");
        assert!(!info[0].is_active);
        assert_eq!(info[1].name, "Layer 1");
        assert!(info[1].is_active);
    }

    #[test]
    fn test_auto_name() {
        let mut stack = LayerStack::new(256, 256);
        let id = stack.add_layer(String::new());
        assert_eq!(stack.layer(id).unwrap().name, format!("Layer {}", id));
    }

    #[test]
    fn test_reorder() {
        let mut stack = LayerStack::new(256, 256);
        let id1 = stack.add_layer("A".to_string());
        let id2 = stack.add_layer("B".to_string());
        // Order: Background(0), A(id1), B(id2)
        // Move B to index 0 (bottom)
        stack.reorder(id2, 0);
        let info = stack.layer_info();
        assert_eq!(info[0].id, id2);
        assert_eq!(info[1].id, 0);
        assert_eq!(info[2].id, id1);
    }
}
