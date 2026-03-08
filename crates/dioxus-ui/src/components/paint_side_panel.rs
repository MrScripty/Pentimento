//! Paint side panel component - shows painting controls when in paint mode

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;
use crate::components::brush_palette::BrushPalette;
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

.color-swatches-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 8px;
}

.color-swatch-btn {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    cursor: pointer;
    padding: 0;
}

.color-swatch-btn:hover {
    border-color: rgba(255, 255, 255, 0.5);
}

.swatches-label {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.35);
    margin-top: 8px;
    margin-bottom: 4px;
}
"#;

/// Preset color swatches
const PRESET_SWATCHES: [[f32; 4]; 8] = [
    [0.0, 0.0, 0.0, 1.0],   // Black
    [1.0, 1.0, 1.0, 1.0],   // White
    [1.0, 0.0, 0.0, 1.0],   // Red
    [0.0, 0.5, 1.0, 1.0],   // Blue
    [0.0, 0.8, 0.2, 1.0],   // Green
    [1.0, 1.0, 0.0, 1.0],   // Yellow
    [0.6, 0.2, 0.8, 1.0],   // Purple
    [1.0, 0.5, 0.0, 1.0],   // Orange
];

/// Maximum number of recent colors to track
const MAX_RECENT_COLORS: usize = 8;

/// Format a color as a CSS hex string
fn color_to_hex(color: &[f32; 4]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
    )
}

#[derive(Props, Clone, PartialEq)]
pub struct PaintSidePanelProps {
    pub bridge: DioxusBridge,
    #[props(default)]
    pub layers: Vec<pentimento_ipc::LayerInfo>,
}

#[component]
pub fn PaintSidePanel(props: PaintSidePanelProps) -> Element {
    // Brush settings state
    let mut brush_color = use_signal(|| [0.0f32, 0.0, 0.0, 1.0]); // Black default
    let mut brush_size = use_signal(|| 20.0f32);
    let mut brush_opacity = use_signal(|| 1.0f32);
    let mut brush_hardness = use_signal(|| 0.8f32);
    let mut active_preset_id = use_signal(|| 0u32);

    // Color history state
    let mut color_history = use_signal(|| Vec::<[f32; 4]>::new());

    // Color change handler
    let bridge = props.bridge.clone();
    let handle_color_change = move |color: [f32; 4]| {
        // Push previous color to history (deduplicated)
        let prev = brush_color();
        let mut history = color_history();
        history.retain(|c| c != &prev);
        history.insert(0, prev);
        history.truncate(MAX_RECENT_COLORS);
        color_history.set(history);

        brush_color.set(color);
        bridge.set_brush_color(color);
    };

    // Swatch click handler
    let bridge_swatch = props.bridge.clone();
    let handle_swatch_click = move |color: [f32; 4]| {
        let prev = brush_color();
        let mut history = color_history();
        history.retain(|c| c != &prev);
        history.insert(0, prev);
        history.truncate(MAX_RECENT_COLORS);
        color_history.set(history);

        brush_color.set(color);
        bridge_swatch.set_brush_color(color);
    };

    // Brush preset selection handler
    let handle_preset_select = move |(id, size, opacity, hardness): (u32, f32, f32, f32)| {
        active_preset_id.set(id);
        brush_size.set(size);
        brush_opacity.set(opacity);
        brush_hardness.set(hardness);
    };

    // Brush size handler
    let bridge_size = props.bridge.clone();
    let handle_size_change = move |value: f32| {
        brush_size.set(value);
        bridge_size.set_brush_size(value);
    };

    // Brush opacity handler
    let bridge_opacity = props.bridge.clone();
    let handle_opacity_change = move |value: f32| {
        let normalized = value / 100.0;
        brush_opacity.set(normalized);
        bridge_opacity.set_brush_opacity(normalized);
    };

    // Brush hardness handler
    let bridge_hardness = props.bridge.clone();
    let handle_hardness_change = move |value: f32| {
        let normalized = value / 100.0;
        brush_hardness.set(normalized);
        bridge_hardness.set_brush_hardness(normalized);
    };

    // Layer handlers
    let bridge_add_layer = props.bridge.clone();
    let handle_add_layer = move |_: Event<MouseData>| {
        bridge_add_layer.add_layer(String::new());
    };

    let bridge_remove_layer = props.bridge.clone();
    let layers_for_delete = props.layers.clone();
    let handle_remove_layer = move |_: Event<MouseData>| {
        if let Some(active) = layers_for_delete.iter().find(|l| l.is_active) {
            bridge_remove_layer.remove_layer(active.id);
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

                // Preset swatches
                div { class: "swatches-label", "Swatches" }
                div { class: "color-swatches-row",
                    for color in PRESET_SWATCHES.iter() {
                        {
                            let c = *color;
                            let hex = color_to_hex(&c);
                            let mut on_click = handle_swatch_click.clone();
                            rsx! {
                                button {
                                    class: "color-swatch-btn",
                                    style: "background-color: {hex};",
                                    onclick: move |_| on_click(c),
                                }
                            }
                        }
                    }
                }

                // Recent colors
                if !color_history().is_empty() {
                    div { class: "swatches-label", "Recent" }
                    div { class: "color-swatches-row",
                        for color in color_history().iter() {
                            {
                                let c = *color;
                                let hex = color_to_hex(&c);
                                let mut on_click = handle_swatch_click.clone();
                                rsx! {
                                    button {
                                        class: "color-swatch-btn",
                                        style: "background-color: {hex};",
                                        onclick: move |_| on_click(c),
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Brush presets section
            section { class: "section",
                h2 { class: "section-title", "Brushes" }
                BrushPalette {
                    bridge: props.bridge.clone(),
                    active_preset_id: active_preset_id(),
                    on_select: handle_preset_select,
                }
            }

            // Brush settings section
            section { class: "section",
                h2 { class: "section-title", "Brush Settings" }

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

            // Layers section
            section { class: "section",
                h2 { class: "section-title", "Layers" }
                crate::components::layers_panel::LayersPanel {
                    bridge: props.bridge.clone(),
                    layers: props.layers.clone(),
                }
            }
        }
    }
}
