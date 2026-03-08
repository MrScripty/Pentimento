//! Brush palette component - grid of brush presets for quick selection

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;

const BRUSH_PALETTE_CSS: &str = r#"
.brush-palette-grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 4px;
}

.brush-preset-btn {
    padding: 6px 4px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    font-size: 11px;
    text-align: center;
    transition: all 0.15s;
}

.brush-preset-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
}

.brush-preset-active {
    background: rgba(100, 150, 255, 0.3);
    border-color: rgba(100, 150, 255, 0.5);
    color: white;
}
"#;

/// Preset info for rendering in the UI
struct PresetInfo {
    id: u32,
    name: &'static str,
    base_size: f32,
    hardness: f32,
    opacity: f32,
}

const PRESETS: &[PresetInfo] = &[
    PresetInfo {
        id: 0,
        name: "Hard Round",
        base_size: 20.0,
        hardness: 1.0,
        opacity: 1.0,
    },
    PresetInfo {
        id: 1,
        name: "Soft Round",
        base_size: 30.0,
        hardness: 0.3,
        opacity: 0.8,
    },
    PresetInfo {
        id: 2,
        name: "Pencil",
        base_size: 8.0,
        hardness: 0.9,
        opacity: 0.9,
    },
    PresetInfo {
        id: 3,
        name: "Airbrush",
        base_size: 40.0,
        hardness: 0.1,
        opacity: 0.3,
    },
    PresetInfo {
        id: 4,
        name: "Ink",
        base_size: 12.0,
        hardness: 1.0,
        opacity: 1.0,
    },
    PresetInfo {
        id: 5,
        name: "Marker",
        base_size: 25.0,
        hardness: 0.6,
        opacity: 0.6,
    },
];

#[derive(Props, Clone, PartialEq)]
pub struct BrushPaletteProps {
    pub bridge: DioxusBridge,
    pub active_preset_id: u32,
    pub on_select: EventHandler<(u32, f32, f32, f32)>, // (id, size, opacity, hardness)
}

#[component]
pub fn BrushPalette(props: BrushPaletteProps) -> Element {
    rsx! {
        style { {BRUSH_PALETTE_CSS} }
        div { class: "brush-palette-grid",
            for preset in PRESETS.iter() {
                {
                    let id = preset.id;
                    let size = preset.base_size;
                    let opacity = preset.opacity;
                    let hardness = preset.hardness;
                    let bridge = props.bridge.clone();
                    let on_select = props.on_select.clone();
                    let class = if props.active_preset_id == id {
                        "brush-preset-btn brush-preset-active"
                    } else {
                        "brush-preset-btn"
                    };
                    rsx! {
                        button {
                            class: class,
                            onclick: move |_| {
                                bridge.select_brush_preset(id);
                                on_select.call((id, size, opacity, hardness));
                            },
                            "{preset.name}"
                        }
                    }
                }
            }
        }
    }
}
