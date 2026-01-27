//! Dab application for brush strokes

use tracing::debug;

use super::TiledSurface;
use crate::types::BlendMode;

impl TiledSurface {
    /// Apply a dab to the surface (basic circle stamp)
    /// Returns bounding box of affected region (x, y, width, height)
    /// Returns None if the dab is completely outside the surface
    ///
    /// This is a convenience wrapper around `apply_dab_ellipse` for circular dabs.
    pub fn apply_dab(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
    ) -> Option<(u32, u32, u32, u32)> {
        // Delegate to ellipse implementation with circular parameters
        self.apply_dab_ellipse(
            center_x,
            center_y,
            radius,
            color,
            opacity,
            hardness,
            blend_mode,
            0.0, // angle
            1.0, // aspect_ratio (1.0 = circle)
        )
    }

    /// Apply an elliptical dab to the surface with rotation support.
    ///
    /// This is the core dab application function that supports both circular and
    /// elliptical brushes with arbitrary rotation. Used for 3D mesh painting where
    /// brushes must be projected onto surfaces at oblique angles.
    ///
    /// # Arguments
    /// * `center_x`, `center_y` - Dab center in pixel coordinates
    /// * `radius` - Radius along the major axis (in pixels)
    /// * `color` - RGBA color to apply
    /// * `opacity` - Overall opacity (0.0 to 1.0)
    /// * `hardness` - Edge hardness (0.0 = soft, 1.0 = hard)
    /// * `blend_mode` - How to combine with existing pixels
    /// * `angle` - Rotation angle in radians (counter-clockwise)
    /// * `aspect_ratio` - Ratio of minor to major axis (1.0 = circle, 0.5 = 2:1 ellipse)
    ///
    /// # Returns
    /// Bounding box of affected region (x, y, width, height), or None if outside surface.
    pub fn apply_dab_ellipse(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
        angle: f32,
        aspect_ratio: f32,
    ) -> Option<(u32, u32, u32, u32)> {
        debug!(
            "TiledSurface::apply_dab_ellipse: center=({:.1}, {:.1}), radius={:.1}, aspect={:.2}, angle={:.2}rad, opacity={:.2}, hardness={:.2}, mode={:?}",
            center_x, center_y, radius, aspect_ratio, angle, opacity, hardness, blend_mode
        );

        if radius <= 0.0 || opacity <= 0.0 || aspect_ratio <= 0.0 {
            debug!("  -> skipped: invalid radius, opacity, or aspect_ratio");
            return None;
        }

        // Clamp aspect ratio to prevent extreme ellipses
        let aspect_ratio = aspect_ratio.clamp(0.01, 1.0);

        // For ellipse bounding box, we need to account for rotation.
        // The bounding box of a rotated ellipse with semi-axes a (major) and b (minor)
        // rotated by angle θ has half-widths:
        //   half_w = sqrt(a² cos²θ + b² sin²θ)
        //   half_h = sqrt(a² sin²θ + b² cos²θ)
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let cos_sq = cos_a * cos_a;
        let sin_sq = sin_a * sin_a;

        let radius_major = radius;
        let radius_minor = radius * aspect_ratio;
        let r_major_sq = radius_major * radius_major;
        let r_minor_sq = radius_minor * radius_minor;

        let half_w = (r_major_sq * cos_sq + r_minor_sq * sin_sq).sqrt();
        let half_h = (r_major_sq * sin_sq + r_minor_sq * cos_sq).sqrt();

        // Calculate bounding box
        let x_min_f = (center_x - half_w).floor();
        let y_min_f = (center_y - half_h).floor();
        let x_max_f = (center_x + half_w).ceil();
        let y_max_f = (center_y + half_h).ceil();

        // Clamp to surface bounds
        let x_min = (x_min_f.max(0.0) as u32).min(self.surface.width);
        let y_min = (y_min_f.max(0.0) as u32).min(self.surface.height);
        let x_max = (x_max_f.max(0.0) as u32).min(self.surface.width);
        let y_max = (y_max_f.max(0.0) as u32).min(self.surface.height);

        // Check if completely outside
        if x_min >= x_max || y_min >= y_max {
            return None;
        }

        // Apply dab to each pixel in the bounding box
        for py in y_min..y_max {
            for px in x_min..x_max {
                // Calculate distance from center (use pixel center)
                let dx = (px as f32 + 0.5) - center_x;
                let dy = (py as f32 + 0.5) - center_y;

                // Rotate point by -angle to align with ellipse axes
                let rotated_x = dx * cos_a + dy * sin_a;
                let rotated_y = -dx * sin_a + dy * cos_a;

                // Normalize to unit circle space (ellipse becomes circle)
                let normalized_x = rotated_x / radius_major;
                let normalized_y = rotated_y / radius_minor;
                let normalized_dist_sq = normalized_x * normalized_x + normalized_y * normalized_y;

                // Skip if outside the ellipse (distance > 1 in normalized space)
                if normalized_dist_sq > 1.0 {
                    continue;
                }

                // Calculate normalized distance (0 at center, 1 at edge)
                let distance_normalized = normalized_dist_sq.sqrt();

                // Calculate falloff based on hardness
                let falloff = calculate_hardness_falloff(distance_normalized, hardness);

                if falloff > 0.0 {
                    let effective_opacity = opacity * falloff;
                    match blend_mode {
                        BlendMode::Normal => {
                            // Blend the color with the calculated falloff
                            self.surface.blend_pixel(px, py, color, effective_opacity);
                        }
                        BlendMode::Erase => {
                            // Erase by reducing alpha
                            self.surface.erase_pixel(px, py, effective_opacity);
                        }
                    }
                }
            }
        }

        // Mark the affected region as dirty
        let width = x_max - x_min;
        let height = y_max - y_min;
        self.mark_region_dirty(x_min, y_min, width, height);

        Some((x_min, y_min, width, height))
    }
}

/// Calculate falloff based on hardness
/// distance_normalized is 0 at center, 1 at edge
/// hardness is 0.0 (soft) to 1.0 (hard)
#[inline]
pub fn calculate_hardness_falloff(distance_normalized: f32, hardness: f32) -> f32 {
    if hardness >= 1.0 {
        // Pure hard edge
        if distance_normalized <= 1.0 {
            1.0
        } else {
            0.0
        }
    } else {
        let t = distance_normalized.clamp(0.0, 1.0);
        let soft = 1.0 - t; // Linear falloff for soft brush
        let hard = if t <= 1.0 { 1.0 } else { 0.0 };
        // Interpolate between soft and hard based on hardness
        soft * (1.0 - hardness) + hard * hardness
    }
}
