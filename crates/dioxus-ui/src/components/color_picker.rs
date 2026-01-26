//! Custom HSV color picker component

use dioxus::prelude::*;

const COLOR_PICKER_CSS: &str = r#"
.color-picker {
    display: flex;
    flex-direction: column;
    gap: 12px;
}

.color-preview-row {
    display: flex;
    align-items: center;
    gap: 12px;
}

.color-swatch {
    width: 48px;
    height: 48px;
    border-radius: 8px;
    border: 2px solid rgba(255, 255, 255, 0.2);
    flex-shrink: 0;
}

.color-values {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.color-value-row {
    display: flex;
    gap: 8px;
    font-size: 11px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.6);
}

.color-value-label {
    width: 16px;
    color: rgba(255, 255, 255, 0.4);
}

.sv-picker {
    position: relative;
    width: 100%;
    height: 150px;
    border-radius: 4px;
    cursor: crosshair;
    overflow: hidden;
}

.sv-picker-saturation {
    position: absolute;
    inset: 0;
    background: linear-gradient(to right, white, transparent);
}

.sv-picker-value {
    position: absolute;
    inset: 0;
    background: linear-gradient(to top, black, transparent);
}

.sv-cursor {
    position: absolute;
    width: 12px;
    height: 12px;
    border: 2px solid white;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    box-shadow: 0 0 4px rgba(0, 0, 0, 0.5);
    pointer-events: none;
}

.hue-slider-container {
    display: flex;
    align-items: center;
    gap: 8px;
}

.hue-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
    width: 32px;
}

.hue-slider {
    flex: 1;
    height: 16px;
    border-radius: 8px;
    background: linear-gradient(to right,
        hsl(0, 100%, 50%),
        hsl(60, 100%, 50%),
        hsl(120, 100%, 50%),
        hsl(180, 100%, 50%),
        hsl(240, 100%, 50%),
        hsl(300, 100%, 50%),
        hsl(360, 100%, 50%)
    );
    appearance: none;
    cursor: pointer;
    border: 1px solid rgba(255, 255, 255, 0.1);
}

.hue-slider::-webkit-slider-thumb {
    appearance: none;
    width: 8px;
    height: 20px;
    background: white;
    border-radius: 4px;
    cursor: pointer;
    box-shadow: 0 0 4px rgba(0, 0, 0, 0.3);
}
"#;

/// Convert HSV to RGB
/// h: 0-360, s: 0-1, v: 0-1
/// Returns [r, g, b, a] with values 0-1
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 4] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    [r + m, g + m, b + m, 1.0]
}

/// Convert RGB to HSV
/// r, g, b: 0-1
/// Returns (h: 0-360, s: 0-1, v: 0-1)
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };

    let s = if max == 0.0 { 0.0 } else { delta / max };

    let v = max;

    (h, s, v)
}

#[derive(Props, Clone, PartialEq)]
pub struct ColorPickerProps {
    /// Current color as [r, g, b, a] with values 0-1
    pub color: [f32; 4],
    /// Callback when color changes
    pub on_change: EventHandler<[f32; 4]>,
}

#[component]
pub fn ColorPicker(props: ColorPickerProps) -> Element {
    // Convert initial color to HSV
    let (init_h, init_s, init_v) = rgb_to_hsv(props.color[0], props.color[1], props.color[2]);

    let mut hue = use_signal(|| init_h);
    let mut saturation = use_signal(|| init_s);
    let mut value = use_signal(|| init_v);
    let mut is_dragging_sv = use_signal(|| false);

    // Compute current RGB color
    let current_rgb = hsv_to_rgb(hue(), saturation(), value());

    // Format color as hex for display
    let hex_color = format!(
        "#{:02X}{:02X}{:02X}",
        (current_rgb[0] * 255.0) as u8,
        (current_rgb[1] * 255.0) as u8,
        (current_rgb[2] * 255.0) as u8
    );

    // Background color for SV picker (pure hue)
    let hue_color = hsv_to_rgb(hue(), 1.0, 1.0);
    let hue_bg = format!(
        "rgb({}, {}, {})",
        (hue_color[0] * 255.0) as u8,
        (hue_color[1] * 255.0) as u8,
        (hue_color[2] * 255.0) as u8
    );

    // Hue slider handler
    let on_change = props.on_change.clone();
    let handle_hue_change = move |evt: Event<FormData>| {
        if let Ok(h) = evt.value().parse::<f32>() {
            hue.set(h);
            let color = hsv_to_rgb(h, saturation(), value());
            on_change.call(color);
        }
    };

    // SV picker mouse down handler
    let on_change = props.on_change.clone();
    let handle_sv_mousedown = move |evt: Event<MouseData>| {
        is_dragging_sv.set(true);

        // Get click position relative to element
        let coords = evt.data().element_coordinates();
        let x = coords.x as f32;
        let y = coords.y as f32;

        // Assuming 268px width (300px panel - 16px padding each side)
        // and 150px height
        let width = 268.0;
        let height = 150.0;

        let s = (x / width).clamp(0.0, 1.0);
        let v = 1.0 - (y / height).clamp(0.0, 1.0);

        saturation.set(s);
        value.set(v);

        let color = hsv_to_rgb(hue(), s, v);
        on_change.call(color);
    };

    // SV picker mouse move handler
    let on_change = props.on_change.clone();
    let handle_sv_mousemove = move |evt: Event<MouseData>| {
        if !is_dragging_sv() {
            return;
        }

        let coords = evt.data().element_coordinates();
        let x = coords.x as f32;
        let y = coords.y as f32;

        let width = 268.0;
        let height = 150.0;

        let s = (x / width).clamp(0.0, 1.0);
        let v = 1.0 - (y / height).clamp(0.0, 1.0);

        saturation.set(s);
        value.set(v);

        let color = hsv_to_rgb(hue(), s, v);
        on_change.call(color);
    };

    // Mouse up handler
    let handle_sv_mouseup = move |_evt: Event<MouseData>| {
        is_dragging_sv.set(false);
    };

    // Cursor position in SV picker
    let cursor_x = saturation() * 100.0;
    let cursor_y = (1.0 - value()) * 100.0;

    rsx! {
        style { {COLOR_PICKER_CSS} }
        div { class: "color-picker",
            // Color preview and values
            div { class: "color-preview-row",
                div {
                    class: "color-swatch",
                    style: "background-color: {hex_color};"
                }
                div { class: "color-values",
                    div { class: "color-value-row",
                        span { class: "color-value-label", "R" }
                        span { "{(current_rgb[0] * 255.0) as u8}" }
                    }
                    div { class: "color-value-row",
                        span { class: "color-value-label", "G" }
                        span { "{(current_rgb[1] * 255.0) as u8}" }
                    }
                    div { class: "color-value-row",
                        span { class: "color-value-label", "B" }
                        span { "{(current_rgb[2] * 255.0) as u8}" }
                    }
                }
            }

            // Saturation/Value picker
            div {
                class: "sv-picker",
                style: "background-color: {hue_bg};",
                onmousedown: handle_sv_mousedown,
                onmousemove: handle_sv_mousemove,
                onmouseup: handle_sv_mouseup,
                onmouseleave: handle_sv_mouseup,

                div { class: "sv-picker-saturation" }
                div { class: "sv-picker-value" }
                div {
                    class: "sv-cursor",
                    style: "left: {cursor_x}%; top: {cursor_y}%;"
                }
            }

            // Hue slider
            div { class: "hue-slider-container",
                span { class: "hue-label", "Hue" }
                input {
                    r#type: "range",
                    min: "0",
                    max: "360",
                    step: "1",
                    value: "{hue}",
                    oninput: handle_hue_change,
                    class: "hue-slider"
                }
            }
        }
    }
}
