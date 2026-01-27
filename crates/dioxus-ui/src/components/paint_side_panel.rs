//! Paint side panel component - shows painting controls when in paint mode

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;
use crate::components::color_picker::ColorPicker;
use crate::components::Slider;

const PAINT_SIDE_PANEL_CSS: &str = r#"
.paint-side-panel {
    /* Normal flow layout - Blitz hit testing doesn't work with position:fixed */
    /* Use flexbox layout from parent to position on right side */
    width: 300px;
    margin: 8px;
    border-radius: 8px;
    overflow-y: auto;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    pointer-events: auto;
    /* position:relative + z-index ensures this is above the keyboard-focus-trap overlay */
    position: relative;
    z-index: 10;
}

.section {
    padding: 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.section:last-child {
    border-bottom: none;
}

.section-title {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: rgba(255, 255, 255, 0.5);
    margin: 0 0 12px 0;
}

.property-group {
    margin-bottom: 16px;
}

.property-group:last-child {
    margin-bottom: 0;
}

.property {
    display: grid;
    grid-template-columns: 80px 1fr 40px;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
}

.property:last-child {
    margin-bottom: 0;
}

.property-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
}

.property-value {
    font-size: 11px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.5);
    text-align: right;
}
"#;

#[derive(Props, Clone, PartialEq)]
pub struct PaintSidePanelProps {
    pub bridge: DioxusBridge,
}

#[component]
pub fn PaintSidePanel(props: PaintSidePanelProps) -> Element {
    // Brush settings state
    let mut brush_color = use_signal(|| [0.0f32, 0.0, 0.0, 1.0]); // Black default
    let mut brush_size = use_signal(|| 20.0f32);
    let mut brush_opacity = use_signal(|| 1.0f32);
    let mut brush_hardness = use_signal(|| 0.8f32);

    // Color change handler
    let bridge = props.bridge.clone();
    let handle_color_change = move |color: [f32; 4]| {
        brush_color.set(color);
        bridge.set_brush_color(color);
    };

    // Brush size handler - takes f32 directly for custom Slider
    let bridge_size = props.bridge.clone();
    let handle_size_change = move |value: f32| {
        brush_size.set(value);
        bridge_size.set_brush_size(value);
    };

    // Brush opacity handler
    let bridge_opacity = props.bridge.clone();
    let handle_opacity_change = move |value: f32| {
        // value is 0-100, normalize to 0-1
        let normalized = value / 100.0;
        brush_opacity.set(normalized);
        bridge_opacity.set_brush_opacity(normalized);
    };

    // Brush hardness handler
    let bridge_hardness = props.bridge.clone();
    let handle_hardness_change = move |value: f32| {
        // value is 0-100, normalize to 0-1
        let normalized = value / 100.0;
        brush_hardness.set(normalized);
        bridge_hardness.set_brush_hardness(normalized);
    };

    rsx! {
        style { {PAINT_SIDE_PANEL_CSS} }
        aside { class: "paint-side-panel panel",
            // Color section
            section { class: "section",
                h2 { class: "section-title", "Color" }
                ColorPicker {
                    color: brush_color(),
                    on_change: handle_color_change
                }
            }

            // Brush settings section
            section { class: "section",
                h2 { class: "section-title", "Brush" }

                div { class: "property-group",
                    div { class: "property",
                        label { class: "property-label", "Size" }
                        Slider {
                            value: brush_size(),
                            min: 1.0,
                            max: 100.0,
                            step: 1.0,
                            on_change: handle_size_change
                        }
                        span { class: "property-value", "{brush_size:.0}px" }
                    }

                    div { class: "property",
                        label { class: "property-label", "Opacity" }
                        Slider {
                            value: brush_opacity() * 100.0,
                            min: 0.0,
                            max: 100.0,
                            step: 1.0,
                            on_change: handle_opacity_change
                        }
                        span { class: "property-value", "{(brush_opacity() * 100.0) as i32}%" }
                    }

                    div { class: "property",
                        label { class: "property-label", "Hardness" }
                        Slider {
                            value: brush_hardness() * 100.0,
                            min: 0.0,
                            max: 100.0,
                            step: 1.0,
                            on_change: handle_hardness_change
                        }
                        span { class: "property-value", "{(brush_hardness() * 100.0) as i32}%" }
                    }
                }
            }
        }
    }
}
