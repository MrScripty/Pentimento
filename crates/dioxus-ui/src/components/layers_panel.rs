//! Layers panel component - shows layer stack with visibility and selection controls

use dioxus::prelude::*;
use pentimento_ipc::LayerInfo;

use crate::bridge::DioxusBridge;

const LAYERS_PANEL_CSS: &str = r#"
.layers-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
}

.layers-header .section-title {
    margin: 0;
}

.layers-actions {
    display: flex;
    gap: 4px;
}

.layer-action-btn {
    width: 24px;
    height: 24px;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
}

.layer-action-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
}

.layer-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
}

.layer-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 4px;
    cursor: pointer;
    background: rgba(255, 255, 255, 0.03);
}

.layer-row:hover {
    background: rgba(255, 255, 255, 0.08);
}

.layer-row-active {
    background: rgba(100, 150, 255, 0.2);
}

.layer-row-active:hover {
    background: rgba(100, 150, 255, 0.25);
}

.layer-visibility {
    width: 20px;
    height: 20px;
    border-radius: 3px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    background: transparent;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    font-size: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    flex-shrink: 0;
}

.layer-visibility:hover {
    background: rgba(255, 255, 255, 0.1);
}

.layer-visibility-hidden {
    color: rgba(255, 255, 255, 0.2);
}

.layer-name {
    flex: 1;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.8);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.layer-opacity {
    font-size: 10px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
}

.layers-empty {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
    text-align: center;
    padding: 12px 0;
}
"#;

#[derive(Props, Clone, PartialEq)]
pub struct LayersPanelProps {
    pub bridge: DioxusBridge,
    pub layers: Vec<LayerInfo>,
}

#[component]
pub fn LayersPanel(props: LayersPanelProps) -> Element {
    let bridge_add = props.bridge.clone();
    let handle_add = move |_| {
        bridge_add.add_layer(String::new());
    };

    let layers_for_delete = props.layers.clone();
    let bridge_delete = props.bridge.clone();
    let handle_delete = move |_| {
        if let Some(active) = layers_for_delete.iter().find(|l| l.is_active) {
            bridge_delete.remove_layer(active.id);
        }
    };

    rsx! {
        style { {LAYERS_PANEL_CSS} }
        div {
            // Header with add/delete buttons
            div { class: "layers-header",
                div { class: "layers-actions",
                    button {
                        class: "layer-action-btn",
                        title: "Add layer",
                        onclick: handle_add,
                        "+"
                    }
                    button {
                        class: "layer-action-btn",
                        title: "Remove active layer",
                        onclick: handle_delete,
                        "-"
                    }
                }
            }

            // Layer list (reversed: top layer displayed first)
            if props.layers.is_empty() {
                div { class: "layers-empty", "No layers" }
            } else {
                div { class: "layer-list",
                    for layer in props.layers.iter().rev() {
                        {
                            let id = layer.id;
                            let visible = layer.visible;
                            let is_active = layer.is_active;
                            let name = layer.name.clone();
                            let opacity = layer.opacity;

                            let bridge_select = props.bridge.clone();
                            let bridge_vis = props.bridge.clone();

                            let row_class = if is_active {
                                "layer-row layer-row-active"
                            } else {
                                "layer-row"
                            };

                            let vis_class = if visible {
                                "layer-visibility"
                            } else {
                                "layer-visibility layer-visibility-hidden"
                            };

                            let vis_text = if visible { "V" } else { "H" };

                            rsx! {
                                div {
                                    class: row_class,
                                    onclick: move |_| {
                                        bridge_select.set_active_layer(id);
                                    },
                                    button {
                                        class: vis_class,
                                        title: if visible { "Hide layer" } else { "Show layer" },
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            bridge_vis.set_layer_visibility(id, !visible);
                                        },
                                        "{vis_text}"
                                    }
                                    span { class: "layer-name", "{name}" }
                                    span { class: "layer-opacity", "{(opacity * 100.0) as i32}%" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
