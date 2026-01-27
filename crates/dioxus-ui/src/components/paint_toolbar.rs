//! Paint toolbar component - shows brush tools when in paint mode

use dioxus::prelude::*;
use pentimento_ipc::BlendMode;

use crate::bridge::DioxusBridge;

const PAINT_TOOLBAR_CSS: &str = r#"
.paint-toolbar {
    position: fixed;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 24px;
    z-index: 200;
}

.tool-group {
    display: flex;
    gap: 4px;
    padding-right: 12px;
    border-right: 1px solid rgba(255, 255, 255, 0.1);
}

.action-group {
    display: flex;
    gap: 4px;
    padding-right: 12px;
    border-right: 1px solid rgba(255, 255, 255, 0.1);
}

.paint-tool {
    width: 36px;
    height: 36px;
    border-radius: 18px;
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 18px;
    transition: all 0.15s;
}

.paint-tool:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
}

.paint-tool-selected {
    background: rgba(100, 150, 255, 0.3);
    color: white;
}

.paint-tool:disabled {
    opacity: 0.3;
    cursor: not-allowed;
}

.toolbar-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    margin-left: 4px;
}

.projection-group {
    display: flex;
    gap: 4px;
    padding-left: 4px;
}

.projection-toggle-active {
    background: rgba(80, 200, 120, 0.3) !important;
    color: white !important;
}
"#;

/// Paint tool types
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PaintTool {
    Brush,
    Eraser,
}

#[derive(Props, Clone, PartialEq)]
pub struct PaintToolbarProps {
    pub visible: bool,
    pub bridge: DioxusBridge,
}

#[component]
pub fn PaintToolbar(props: PaintToolbarProps) -> Element {
    if !props.visible {
        return rsx! {};
    }

    // Current tool state
    let mut current_tool = use_signal(|| PaintTool::Brush);
    // Live projection state
    let mut live_projection = use_signal(|| false);

    // Tool selection handlers
    let bridge = props.bridge.clone();
    let handle_brush_click = move |_| {
        current_tool.set(PaintTool::Brush);
        bridge.set_blend_mode(BlendMode::Normal);
    };

    let bridge = props.bridge.clone();
    let handle_eraser_click = move |_| {
        current_tool.set(PaintTool::Eraser);
        bridge.set_blend_mode(BlendMode::Erase);
    };

    // Undo handler
    let bridge = props.bridge.clone();
    let handle_undo_click = move |_| {
        bridge.paint_undo();
    };

    // Live projection toggle handler
    let bridge = props.bridge.clone();
    let handle_live_projection_toggle = move |_| {
        let new_state = !live_projection();
        live_projection.set(new_state);
        bridge.set_live_projection(new_state);
    };

    // Project to scene handler
    let bridge = props.bridge.clone();
    let handle_project_to_scene = move |_| {
        bridge.project_to_scene();
    };

    rsx! {
        style { {PAINT_TOOLBAR_CSS} }
        div { class: "paint-toolbar panel",
            // Tool selection group
            div { class: "tool-group",
                if current_tool() == PaintTool::Brush {
                    button {
                        class: "paint-tool paint-tool-selected",
                        title: "Brush (B)",
                        onclick: handle_brush_click,
                        "B"
                    }
                } else {
                    button {
                        class: "paint-tool",
                        title: "Brush (B)",
                        onclick: handle_brush_click,
                        "B"
                    }
                }
                if current_tool() == PaintTool::Eraser {
                    button {
                        class: "paint-tool paint-tool-selected",
                        title: "Eraser (E)",
                        onclick: handle_eraser_click,
                        "E"
                    }
                } else {
                    button {
                        class: "paint-tool",
                        title: "Eraser (E)",
                        onclick: handle_eraser_click,
                        "E"
                    }
                }
            }

            // Action group
            div { class: "action-group",
                button {
                    class: "paint-tool",
                    title: "Undo (Ctrl+Z)",
                    onclick: handle_undo_click,
                    "↶"
                }
            }

            // Projection group
            div { class: "projection-group",
                if live_projection() {
                    button {
                        class: "paint-tool projection-toggle-active",
                        title: "Live Projection (painting projects to meshes in real-time)",
                        onclick: handle_live_projection_toggle,
                        "L"
                    }
                } else {
                    button {
                        class: "paint-tool",
                        title: "Live Projection (painting projects to meshes in real-time)",
                        onclick: handle_live_projection_toggle,
                        "L"
                    }
                }
                button {
                    class: "paint-tool",
                    title: "Project to Scene (apply canvas paint to meshes)",
                    onclick: handle_project_to_scene,
                    "P"
                }
            }

            span { class: "toolbar-hint", "Tab to exit" }
        }
    }
}
