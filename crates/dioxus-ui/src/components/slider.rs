//! Custom slider component for Blitz (which doesn't support native range inputs)
//!
//! Note: Blitz's `element_coordinates()` is a stub that always returns (0, 0).
//! We use a combined approach:
//! - `onmounted` + `get_client_rect()` to get element bounds
//! - Click-to-position when bounds are available
//! - Delta-based dragging for smooth movement regardless of bounds accuracy
//!
//! To handle dragging outside the slider element, we render a fullscreen invisible
//! overlay during drag operations that captures all mouse events. This works around
//! Blitz not supporting pointer capture.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SliderProps {
    /// Current value
    pub value: f32,
    /// Minimum value
    pub min: f32,
    /// Maximum value
    pub max: f32,
    /// Step increment (default 0.01)
    #[props(default = 0.01)]
    pub step: f32,
    /// Callback when value changes
    pub on_change: EventHandler<f32>,
}

#[component]
pub fn Slider(props: SliderProps) -> Element {
    let mut is_dragging = use_signal(|| false);

    // For click-to-position: element bounds from onmounted
    let mut element_bounds = use_signal(|| Option::<(f64, f64)>::None); // (left, width)

    // For delta-based dragging: track where drag started and what value it started at
    let mut drag_start_x = use_signal(|| 0.0f64);
    let mut drag_start_value = use_signal(|| 0.0f32);

    // Calculate thumb position as percentage
    let range = props.max - props.min;
    let normalized = if range > 0.0 {
        ((props.value - props.min) / range).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_percent = normalized * 100.0;

    // Mouse down - start dragging, try to jump to clicked position
    let on_change = props.on_change.clone();
    let min = props.min;
    let max = props.max;
    let step = props.step;
    let current_value = props.value;
    let handle_mousedown = move |evt: Event<MouseData>| {
        let client_x = evt.data().client_coordinates().x;
        tracing::info!("Slider mousedown: bounds={:?}, client_x={}", element_bounds(), client_x);

        // Start dragging
        is_dragging.set(true);
        drag_start_x.set(client_x);

        // Try to jump to clicked position if we have valid bounds
        if let Some((left, width)) = element_bounds() {
            if width > 0.0 {
                let relative_x = client_x - left;
                let normalized = (relative_x / width).clamp(0.0, 1.0) as f32;
                let raw_value = min + normalized * (max - min);
                let stepped = (raw_value / step).round() * step;
                let new_value = stepped.clamp(min, max);
                on_change.call(new_value);
                // Update drag start value to the new jumped-to value
                drag_start_value.set(new_value);
                return;
            }
        }

        // No valid bounds yet - drag will use deltas from current value
        drag_start_value.set(current_value);
    };

    // Mouse move handler for overlay - use deltas for smooth dragging
    let on_change = props.on_change.clone();
    let min = props.min;
    let max = props.max;
    let step = props.step;
    let handle_overlay_mousemove = move |evt: Event<MouseData>| {
        let client_x = evt.data().client_coordinates().x;
        let delta_x = client_x - drag_start_x();

        // Get width from bounds, filtering out zero values, with fallback
        // Blitz's get_client_rect() often returns width=0, so we need the fallback
        let width = element_bounds()
            .map(|(_, w)| w)
            .filter(|w| *w > 0.0)
            .unwrap_or(200.0);

        let delta_normalized = (delta_x / width) as f32;
        let delta_value = delta_normalized * (max - min);
        let new_value = drag_start_value() + delta_value;
        let stepped = (new_value / step).round() * step;
        tracing::debug!("Slider drag: delta_x={}, width={}, new_value={}", delta_x, width, stepped);
        on_change.call(stepped.clamp(min, max));
    };

    // Mouse up on overlay - stop dragging
    let handle_overlay_mouseup = move |_evt: Event<MouseData>| {
        is_dragging.set(false);
    };

    // Click handler - same as mousedown for direct clicks
    let on_change = props.on_change.clone();
    let min = props.min;
    let max = props.max;
    let step = props.step;
    let handle_click = move |evt: Event<MouseData>| {
        // Only process click if not currently dragging (prevents double-handling)
        if is_dragging() {
            return;
        }

        if let Some((left, width)) = element_bounds() {
            if width > 0.0 {
                let client_x = evt.data().client_coordinates().x;
                let relative_x = client_x - left;
                let normalized = (relative_x / width).clamp(0.0, 1.0) as f32;
                let raw_value = min + normalized * (max - min);
                let stepped = (raw_value / step).round() * step;
                on_change.call(stepped.clamp(min, max));
            }
        }
    };

    rsx! {
        // Fullscreen invisible overlay to capture mouse events during drag
        // This ensures dragging continues even when the cursor leaves the slider element
        if is_dragging() {
            div {
                style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; z-index: 9999; cursor: pointer;",
                onmousemove: handle_overlay_mousemove,
                onmouseup: handle_overlay_mouseup,
            }
        }
        button {
            class: "slider-track",
            style: "position: relative; width: 100%; height: 16px; background: rgba(255, 255, 255, 0.1); border: none; border-radius: 2px; cursor: pointer; padding: 0;",
            // Get element bounds on mount using Blitz's get_client_rect()
            onmounted: move |evt| async move {
                let result = evt.data().get_client_rect().await;
                tracing::info!("Slider get_client_rect: {:?}", result);
                if let Ok(rect) = result {
                    tracing::info!("Slider bounds: left={}, width={}", rect.origin.x, rect.size.width);
                    element_bounds.set(Some((rect.origin.x, rect.size.width)));
                }
            },
            onclick: handle_click,
            onmousedown: handle_mousedown,
            div {
                class: "slider-thumb",
                style: "position: absolute; width: 12px; height: 12px; background: white; border-radius: 50%; top: 50%; transform: translate(-50%, -50%); pointer-events: none; box-shadow: 0 0 4px rgba(0, 0, 0, 0.3); left: {thumb_percent}%;"
            }
        }
    }
}
