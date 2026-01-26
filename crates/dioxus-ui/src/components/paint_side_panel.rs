//! Paint side panel component - shows painting controls when in paint mode

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;
use crate::components::color_picker::ColorPicker;

const PAINT_SIDE_PANEL_CSS: &str = r#"
.paint-side-panel {
    position: fixed;
    top: 56px;
    right: 8px;
    bottom: 8px;
    width: 300px;
    border-radius: 8px;
    overflow-y: auto;
    z-index: 50;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
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

.slider {
    width: 100%;
    height: 4px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    appearance: none;
    cursor: pointer;
}

.slider::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    background: white;
    border-radius: 50%;
    cursor: pointer;
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

    // Brush size handler
    let bridge = props.bridge.clone();
    let handle_size_change = move |evt: Event<FormData>| {
        if let Ok(value) = evt.value().parse::<f32>() {
            brush_size.set(value);
            bridge.set_brush_size(value);
        }
    };

    // Brush opacity handler
    let bridge = props.bridge.clone();
    let handle_opacity_change = move |evt: Event<FormData>| {
        if let Ok(value) = evt.value().parse::<f32>() {
            let normalized = value / 100.0;
            brush_opacity.set(normalized);
            bridge.set_brush_opacity(normalized);
        }
    };

    // Brush hardness handler
    let bridge = props.bridge.clone();
    let handle_hardness_change = move |evt: Event<FormData>| {
        if let Ok(value) = evt.value().parse::<f32>() {
            let normalized = value / 100.0;
            brush_hardness.set(normalized);
            bridge.set_brush_hardness(normalized);
        }
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
                        input {
                            r#type: "range",
                            min: "1",
                            max: "100",
                            step: "1",
                            value: "{brush_size}",
                            oninput: handle_size_change,
                            class: "slider"
                        }
                        span { class: "property-value", "{brush_size:.0}px" }
                    }

                    div { class: "property",
                        label { class: "property-label", "Opacity" }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            step: "1",
                            value: "{brush_opacity() * 100.0}",
                            oninput: handle_opacity_change,
                            class: "slider"
                        }
                        span { class: "property-value", "{(brush_opacity() * 100.0) as i32}%" }
                    }

                    div { class: "property",
                        label { class: "property-label", "Hardness" }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            step: "1",
                            value: "{brush_hardness() * 100.0}",
                            oninput: handle_hardness_change,
                            class: "slider"
                        }
                        span { class: "property-value", "{(brush_hardness() * 100.0) as i32}%" }
                    }
                }
            }
        }
    }
}
