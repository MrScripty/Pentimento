//! Toolbar component - replicates the Svelte Toolbar.svelte

use dioxus::prelude::*;
use pentimento_ipc::{EditMode, MeshSelectionMode};

use crate::bridge::{DioxusBridge, SharedUiState};
use crate::state::RenderStats;

const TOOLBAR_CSS: &str = r#"
.toolbar {
    /* Normal flow layout - no positioning for Blitz hit testing compatibility */
    width: 100%;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.toolbar-left,
.toolbar-center,
.toolbar-right {
    display: flex;
    align-items: center;
    gap: 16px;
}

.title {
    font-size: 16px;
    font-weight: 600;
    color: white;
    margin: 0;
}

.nav {
    display: flex;
    gap: 4px;
}

.nav-button {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.8);
    padding: 6px 12px;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
    transition: background 0.15s;
}

.nav-button:hover,
.nav-button.active {
    background: rgba(255, 255, 255, 0.1);
    color: white;
}

.menu-container {
    position: relative;
}

.dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    margin-top: 4px;
    min-width: 160px;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 4px;
    z-index: 200;
}

.dropdown-item {
    display: block;
    width: 100%;
    padding: 8px 12px;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.9);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
    transition: background 0.1s;
}

.dropdown-item:hover {
    background: rgba(255, 255, 255, 0.1);
}

.dropdown-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.1);
    margin: 4px 0;
}

.tool-group {
    position: relative;  /* Required for Blitz hit testing */
    display: flex;
    gap: 2px;
    background: rgba(0, 0, 0, 0.3);
    padding: 4px;
    border-radius: 6px;
}

.tool-button {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.7);
    width: 32px;
    height: 32px;
    border-radius: 4px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
    font-size: 16px;  /* Icon styling moved here */
}

.tool-button:hover {
    background: rgba(255, 255, 255, 0.15);
    color: white;
}

.tool-button-selected {
    background: rgba(100, 150, 255, 0.3);
    border: none;
    color: white;
    width: 32px;
    height: 32px;
    border-radius: 4px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
}

/* .icon class removed - icons styled directly on .tool-button */

.stats {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    font-family: monospace;
}

.stat {
    min-width: 60px;
}

/* Backdrop overlay when menu is open - covers entire viewport */
.toolbar-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;  /* Above normal content, below dropdowns (z-index: 200) */
}

/* Edit mode indicator */
.edit-mode-indicator {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    background: rgba(255, 150, 50, 0.2);
    border: 1px solid rgba(255, 150, 50, 0.4);
    border-radius: 4px;
    font-size: 12px;
    color: rgba(255, 200, 150, 0.9);
}

.edit-mode-label {
    font-weight: 600;
}

/* Selection mode buttons */
.selection-mode-group {
    display: flex;
    gap: 2px;
    background: rgba(0, 0, 0, 0.3);
    padding: 2px;
    border-radius: 4px;
    margin-left: 8px;
}

.selection-mode-button {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.6);
    padding: 4px 8px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 11px;
    transition: all 0.15s;
}

.selection-mode-button:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
}

.selection-mode-button-selected {
    background: rgba(255, 150, 50, 0.3);
    border: none;
    color: white;
    padding: 4px 8px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 11px;
}

.selection-count {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    margin-left: 8px;
}
"#;

#[derive(Props, Clone, PartialEq)]
pub struct ToolbarProps {
    pub render_stats: RenderStats,
    pub bridge: DioxusBridge,
    /// Menu state - controlled by parent for cross-component coordination
    pub open_menu: Signal<Option<String>>,
    /// Shared UI state from Bevy (edit mode, selection, etc.)
    pub shared_state: SharedUiState,
}

#[component]
pub fn Toolbar(props: ToolbarProps) -> Element {
    let mut open_menu = props.open_menu;
    let mut selected_tool = use_signal(|| "select".to_string());
    let shared_state = props.shared_state.clone();

    let bridge = props.bridge.clone();
    let bridge_for_reset = bridge.clone();
    let handle_reset_camera = move |_| {
        bridge_for_reset.camera_reset();
    };

    // Determine edit mode display
    let edit_mode_text = match shared_state.edit_mode {
        EditMode::None => "Object Mode",
        EditMode::Paint => "Paint Mode",
        EditMode::MeshEdit => "Edit Mode",
    };

    let in_mesh_edit = shared_state.edit_mode == EditMode::MeshEdit;

    // Selection count text
    let selection_count = if in_mesh_edit {
        let total = shared_state.selected_vertex_count
            + shared_state.selected_edge_count
            + shared_state.selected_face_count;
        if total > 0 {
            format!("{} selected", total)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    rsx! {
        style { {TOOLBAR_CSS} }
        // Backdrop to catch clicks outside menu
        if open_menu().is_some() {
            div {
                class: "toolbar-menu-backdrop",
                onclick: move |_| open_menu.set(None),
            }
        }
        header { class: "toolbar panel",
            div { class: "toolbar-left",
                h1 { class: "title", "Pentimento" }
                nav { class: "nav",
                    // File menu
                    div { class: "menu-container",
                        button {
                            class: if open_menu() == Some("file".to_string()) { "nav-button active" } else { "nav-button" },
                            onclick: move |_| {
                                tracing::info!("File button clicked, current: {:?}", open_menu());
                                if open_menu() == Some("file".to_string()) {
                                    open_menu.set(None);
                                } else {
                                    open_menu.set(Some("file".to_string()));
                                }
                            },
                            "File"
                        }
                        if open_menu() == Some("file".to_string()) {
                            div {
                                class: "dropdown",
                                onclick: move |e| e.stop_propagation(),
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "New Project" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Open..." }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Save" }
                                div { class: "dropdown-divider" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Export..." }
                            }
                        }
                    }
                    // Edit menu
                    div { class: "menu-container",
                        button {
                            class: if open_menu() == Some("edit".to_string()) { "nav-button active" } else { "nav-button" },
                            onclick: move |_| {
                                tracing::info!("Edit button clicked, current: {:?}", open_menu());
                                if open_menu() == Some("edit".to_string()) {
                                    open_menu.set(None);
                                } else {
                                    open_menu.set(Some("edit".to_string()));
                                }
                            },
                            "Edit"
                        }
                        if open_menu() == Some("edit".to_string()) {
                            div {
                                class: "dropdown",
                                onclick: move |e| e.stop_propagation(),
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Undo" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Redo" }
                                div { class: "dropdown-divider" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Cut" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Copy" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Paste" }
                            }
                        }
                    }
                    // View menu
                    div { class: "menu-container",
                        button {
                            class: if open_menu() == Some("view".to_string()) { "nav-button active" } else { "nav-button" },
                            onclick: move |_| {
                                if open_menu() == Some("view".to_string()) {
                                    open_menu.set(None);
                                } else {
                                    open_menu.set(Some("view".to_string()));
                                }
                            },
                            "View"
                        }
                        if open_menu() == Some("view".to_string()) {
                            div {
                                class: "dropdown",
                                onclick: move |e| e.stop_propagation(),
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Zoom In" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Zoom Out" }
                                button { class: "dropdown-item", onclick: move |_| open_menu.set(None), "Fit to Window" }
                            }
                        }
                    }
                }

                // Edit mode indicator (show when not in object mode)
                if in_mesh_edit {
                    div { class: "edit-mode-indicator",
                        span { class: "edit-mode-label", "{edit_mode_text}" }

                        // Selection mode buttons
                        div { class: "selection-mode-group",
                            if shared_state.mesh_selection_mode == MeshSelectionMode::Vertex {
                                button {
                                    class: "selection-mode-button-selected",
                                    title: "Vertex Mode (1)",
                                    "Vert"
                                }
                            } else {
                                button {
                                    class: "selection-mode-button",
                                    title: "Vertex Mode (1)",
                                    onclick: {
                                        let bridge = bridge.clone();
                                        move |_| bridge.set_mesh_selection_mode(MeshSelectionMode::Vertex)
                                    },
                                    "Vert"
                                }
                            }
                            if shared_state.mesh_selection_mode == MeshSelectionMode::Edge {
                                button {
                                    class: "selection-mode-button-selected",
                                    title: "Edge Mode (2)",
                                    "Edge"
                                }
                            } else {
                                button {
                                    class: "selection-mode-button",
                                    title: "Edge Mode (2)",
                                    onclick: {
                                        let bridge = bridge.clone();
                                        move |_| bridge.set_mesh_selection_mode(MeshSelectionMode::Edge)
                                    },
                                    "Edge"
                                }
                            }
                            if shared_state.mesh_selection_mode == MeshSelectionMode::Face {
                                button {
                                    class: "selection-mode-button-selected",
                                    title: "Face Mode (3)",
                                    "Face"
                                }
                            } else {
                                button {
                                    class: "selection-mode-button",
                                    title: "Face Mode (3)",
                                    onclick: {
                                        let bridge = bridge.clone();
                                        move |_| bridge.set_mesh_selection_mode(MeshSelectionMode::Face)
                                    },
                                    "Face"
                                }
                            }
                        }

                        // Selection count
                        if !selection_count.is_empty() {
                            span { class: "selection-count", "{selection_count}" }
                        }
                    }
                }
            }

            div { class: "toolbar-center",
                div { class: "tool-group",
                    // Use structural changes (if blocks) instead of attribute changes
                    // Blitz handles DOM add/remove but not attribute updates
                    if selected_tool() == "select" {
                        button {
                            class: "tool-button-selected",
                            title: "Select",
                            onclick: move |_| {
                                tracing::info!("Select tool clicked");
                                selected_tool.set("select".to_string());
                            },
                            "↖"
                        }
                    } else {
                        button {
                            class: "tool-button",
                            title: "Select",
                            onclick: move |_| {
                                tracing::info!("Select tool clicked");
                                selected_tool.set("select".to_string());
                            },
                            "↖"
                        }
                    }
                    if selected_tool() == "move" {
                        button {
                            class: "tool-button-selected",
                            title: "Move",
                            onclick: move |_| {
                                tracing::info!("Move tool clicked");
                                selected_tool.set("move".to_string());
                            },
                            "✥"
                        }
                    } else {
                        button {
                            class: "tool-button",
                            title: "Move",
                            onclick: move |_| {
                                tracing::info!("Move tool clicked");
                                selected_tool.set("move".to_string());
                            },
                            "✥"
                        }
                    }
                    if selected_tool() == "rotate" {
                        button {
                            class: "tool-button-selected",
                            title: "Rotate",
                            onclick: move |_| {
                                tracing::info!("Rotate tool clicked");
                                selected_tool.set("rotate".to_string());
                            },
                            "↻"
                        }
                    } else {
                        button {
                            class: "tool-button",
                            title: "Rotate",
                            onclick: move |_| {
                                tracing::info!("Rotate tool clicked");
                                selected_tool.set("rotate".to_string());
                            },
                            "↻"
                        }
                    }
                    if selected_tool() == "scale" {
                        button {
                            class: "tool-button-selected",
                            title: "Scale",
                            onclick: move |_| {
                                tracing::info!("Scale tool clicked");
                                selected_tool.set("scale".to_string());
                            },
                            "⤢"
                        }
                    } else {
                        button {
                            class: "tool-button",
                            title: "Scale",
                            onclick: move |_| {
                                tracing::info!("Scale tool clicked");
                                selected_tool.set("scale".to_string());
                            },
                            "⤢"
                        }
                    }
                }
            }

            div { class: "toolbar-right",
                button {
                    class: "nav-button",
                    onclick: handle_reset_camera,
                    "Reset Camera"
                }
                div { class: "stats",
                    span { class: "stat", "{props.render_stats.fps:.0} FPS" }
                    span { class: "stat", "{props.render_stats.frame_time:.1}ms" }
                }
            }
        }
    }
}
