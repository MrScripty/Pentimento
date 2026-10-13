//! CEF wheel conversion without requiring a CEF runtime for its tests.

/// Convert Pentimento's routed wheel pixels to CEF's native wheel convention.
/// The input router preserves native horizontal direction but negates vertical
/// motion for DOM-style deltaY (positive means down). CEF expects positive up.
/// See the official cefclient Linux OSR `ScrollEvent` implementation:
/// https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/browser/browser_window_osr_gtk.cc
pub fn wheel_deltas(delta_x: f32, delta_y: f32) -> (i32, i32) {
    (delta_x as i32, (-delta_y) as i32)
}

#[cfg(test)]
mod tests {
    use super::wheel_deltas;

    #[test]
    fn vertical_wheel_matches_cef_linux_osr_directions() {
        // X11 button 5 is Winit y=-1; the router scales by 40 and negates.
        assert_eq!(wheel_deltas(0.0, 40.0), (0, -40)); // down
        assert_eq!(wheel_deltas(0.0, -40.0), (0, 40)); // up
    }

    #[test]
    fn horizontal_direction_and_pixel_magnitude_are_preserved() {
        assert_eq!(wheel_deltas(40.0, 0.0), (40, 0)); // left
        assert_eq!(wheel_deltas(-40.0, 0.0), (-40, 0)); // right
        assert_eq!(wheel_deltas(7.0, 13.0), (7, -13));
        assert_eq!(wheel_deltas(-7.0, -13.0), (-7, 13));
        assert_eq!(wheel_deltas(12.5, -12.5), (12, 12));
        assert_eq!(wheel_deltas(-12.5, 12.5), (-12, -12));
        assert_eq!(wheel_deltas(0.0, 0.0), (0, 0));
    }

    #[test]
    fn extreme_vertical_delta_is_negated_before_the_saturating_cast() {
        assert_eq!(wheel_deltas(0.0, i32::MIN as f32), (0, i32::MAX));
    }
}
