//! CPU surface for painting - 16-bit RGBA compatible storage

/// A 16-bit RGBA CPU surface for painting
/// Stores pixels as [f32; 4] (Rgba16Float compatible)
pub struct CpuSurface {
    /// Surface dimensions
    pub width: u32,
    pub height: u32,
    /// Pixel data in row-major order, each pixel is [r, g, b, a] as f32
    pixels: Vec<[f32; 4]>,
}

impl CpuSurface {
    /// Create a new surface with the given dimensions, initialized to transparent black
    pub fn new(width: u32, height: u32) -> Self {
        let pixel_count = (width as usize) * (height as usize);
        Self {
            width,
            height,
            pixels: vec![[0.0, 0.0, 0.0, 0.0]; pixel_count],
        }
    }

    /// Clear the surface to a solid color
    pub fn clear(&mut self, color: [f32; 4]) {
        self.pixels.fill(color);
    }

    /// Get a pixel at the given coordinates
    /// Returns None if coordinates are out of bounds
    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y as usize) * (self.width as usize) + (x as usize);
        Some(self.pixels[index])
    }

    /// Set a pixel at the given coordinates
    /// Does nothing if coordinates are out of bounds
    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [f32; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = (y as usize) * (self.width as usize) + (x as usize);
        self.pixels[index] = color;
    }

    /// Blend a color onto an existing pixel using alpha compositing
    /// Formula: out = src * alpha + dst * (1 - alpha)
    #[inline]
    pub fn blend_pixel(&mut self, x: u32, y: u32, color: [f32; 4], opacity: f32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = (y as usize) * (self.width as usize) + (x as usize);
        let dst = self.pixels[index];

        // Source alpha is the color's alpha multiplied by opacity
        let src_alpha = color[3] * opacity;
        let inv_src_alpha = 1.0 - src_alpha;

        // Standard alpha compositing (premultiplied would be different)
        self.pixels[index] = [
            color[0] * src_alpha + dst[0] * inv_src_alpha,
            color[1] * src_alpha + dst[1] * inv_src_alpha,
            color[2] * src_alpha + dst[2] * inv_src_alpha,
            src_alpha + dst[3] * inv_src_alpha,
        ];
    }

    /// Erase a pixel by reducing its alpha
    /// The erase_amount (0-1) determines how much alpha is removed
    #[inline]
    pub fn erase_pixel(&mut self, x: u32, y: u32, erase_amount: f32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = (y as usize) * (self.width as usize) + (x as usize);
        let dst = self.pixels[index];

        // Reduce alpha by the erase amount
        // Also fade the color to transparent (destination-out compositing)
        let remaining = (1.0 - erase_amount).max(0.0);
        self.pixels[index] = [
            dst[0] * remaining,
            dst[1] * remaining,
            dst[2] * remaining,
            dst[3] * remaining,
        ];
    }

    /// Get raw pixel data for GPU upload
    /// Returns the pixel data as a byte slice suitable for wgpu texture upload
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }

    /// Get the total number of pixels
    #[inline]
    pub fn pixel_count(&self) -> usize {
        self.pixels.len()
    }

    /// Get direct access to pixel data (for advanced operations)
    #[inline]
    pub fn pixels(&self) -> &[[f32; 4]] {
        &self.pixels
    }

    /// Get mutable access to pixel data (for advanced operations)
    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [[f32; 4]] {
        &mut self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_surface() {
        let surface = CpuSurface::new(100, 100);
        assert_eq!(surface.width, 100);
        assert_eq!(surface.height, 100);
        assert_eq!(surface.pixel_count(), 10000);
    }

    #[test]
    fn test_get_set_pixel() {
        let mut surface = CpuSurface::new(10, 10);
        let color = [1.0, 0.5, 0.25, 1.0];

        surface.set_pixel(5, 5, color);
        assert_eq!(surface.get_pixel(5, 5), Some(color));

        // Out of bounds should return None
        assert_eq!(surface.get_pixel(100, 100), None);
    }

    #[test]
    fn test_clear() {
        let mut surface = CpuSurface::new(10, 10);
        let white = [1.0, 1.0, 1.0, 1.0];

        surface.clear(white);

        for y in 0..10 {
            for x in 0..10 {
                assert_eq!(surface.get_pixel(x, y), Some(white));
            }
        }
    }

    #[test]
    fn test_blend_pixel() {
        let mut surface = CpuSurface::new(10, 10);

        // Start with white background
        surface.clear([1.0, 1.0, 1.0, 1.0]);

        // Blend 50% opaque red
        surface.blend_pixel(5, 5, [1.0, 0.0, 0.0, 1.0], 0.5);

        let result = surface.get_pixel(5, 5).unwrap();
        // Should be approximately [1.0, 0.5, 0.5, 1.0]
        assert!((result[0] - 1.0).abs() < 0.01);
        assert!((result[1] - 0.5).abs() < 0.01);
        assert!((result[2] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_as_bytes() {
        let surface = CpuSurface::new(2, 2);
        let bytes = surface.as_bytes();
        // 4 pixels * 4 components * 4 bytes per f32 = 64 bytes
        assert_eq!(bytes.len(), 64);
    }
}
