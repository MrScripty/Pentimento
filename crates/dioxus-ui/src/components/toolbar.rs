//! Toolbar component - replicates the Svelte Toolbar.svelte

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;
use crate::state::RenderStats;

const TOOLBAR_CSS: &str = r#"
.toolbar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    z-index: 100;
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
    background: transparent;
    border: none;
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
}

.tool-button:hover {
    background: rgba(255, 255, 255, 0.15);
    color: white;
}

.tool-button.selected {
    background: rgba(100, 150, 255, 0.3);
    color: white;
}

.icon {
    font-size: 16px;
}

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
"#;

#[derive(Props, Clone, PartialEq)]
pub struct ToolbarProps {
    pub render_stats: RenderStats,
    pub bridge: DioxusBridge,
}

#[component]
pub fn Toolbar(props: ToolbarProps) -> Element {
    let mut open_menu = use_signal(|| None::<String>);
    let mut selected_tool = use_signal(|| "select".to_string());

    let mut toggle_menu = move |menu: &str| {
        let menu = menu.to_string();
        if open_menu() == Some(menu.clone()) {
            open_menu.set(None);
        } else {
            open_menu.set(Some(menu));
        }
    };

    let close_menu = move |_| {
        open_menu.set(None);
    };

    let mut select_tool = move |tool: &str| {
        selected_tool.set(tool.to_string());
    };

    let bridge = props.bridge.clone();
    let handle_reset_camera = move |_| {
        bridge.camera_reset();
    };

    rsx! {
        style { {TOOLBAR_CSS} }
        header { class: "toolbar panel",
            div { class: "toolbar-left",
                h1 { class: "title", "Pentimento" }
                nav { class: "nav",
                    // File menu
                    div { class: "menu-container",
                        button {
                            class: if open_menu() == Some("file".to_string()) { "nav-button active" } else { "nav-button" },
                            onclick: move |_| toggle_menu("file"),
                            "File"
                        }
                        if open_menu() == Some("file".to_string()) {
                            div { class: "dropdown",
                                button { class: "dropdown-item", onclick: close_menu, "New Project" }
                                button { class: "dropdown-item", onclick: close_menu, "Open..." }
                                button { class: "dropdown-item", onclick: close_menu, "Save" }
                                div { class: "dropdown-divider" }
                                button { class: "dropdown-item", onclick: close_menu, "Export..." }
                            }
                        }
                    }
                    // Edit menu
                    div { class: "menu-container",
                        button {
                            class: if open_menu() == Some("edit".to_string()) { "nav-button active" } else { "nav-button" },
                            onclick: move |_| toggle_menu("edit"),
                            "Edit"
                        }
                        if open_menu() == Some("edit".to_string()) {
                            div { class: "dropdown",
                                button { class: "dropdown-item", onclick: close_menu, "Undo" }
                                button { class: "dropdown-item", onclick: close_menu, "Redo" }
                                div { class: "dropdown-divider" }
                                button { class: "dropdown-item", onclick: close_menu, "Cut" }
                                button { class: "dropdown-item", onclick: close_menu, "Copy" }
                                button { class: "dropdown-item", onclick: close_menu, "Paste" }
                            }
                        }
                    }
                    // View menu
                    div { class: "menu-container",
                        button {
                            class: if open_menu() == Some("view".to_string()) { "nav-button active" } else { "nav-button" },
                            onclick: move |_| toggle_menu("view"),
                            "View"
                        }
                        if open_menu() == Some("view".to_string()) {
                            div { class: "dropdown",
                                button { class: "dropdown-item", onclick: close_menu, "Zoom In" }
                                button { class: "dropdown-item", onclick: close_menu, "Zoom Out" }
                                button { class: "dropdown-item", onclick: close_menu, "Fit to Window" }
                            }
                        }
                    }
                }
            }

            div { class: "toolbar-center",
                div { class: "tool-group",
                    button {
                        class: if selected_tool() == "select" { "tool-button selected" } else { "tool-button" },
                        title: "Select",
                        onclick: move |_| select_tool("select"),
                        span { class: "icon", "↖" }
                    }
                    button {
                        class: if selected_tool() == "move" { "tool-button selected" } else { "tool-button" },
                        title: "Move",
                        onclick: move |_| select_tool("move"),
                        span { class: "icon", "✥" }
                    }
                    button {
                        class: if selected_tool() == "rotate" { "tool-button selected" } else { "tool-button" },
                        title: "Rotate",
                        onclick: move |_| select_tool("rotate"),
                        span { class: "icon", "↻" }
                    }
                    button {
                        class: if selected_tool() == "scale" { "tool-button selected" } else { "tool-button" },
                        title: "Scale",
                        onclick: move |_| select_tool("scale"),
                        span { class: "icon", "⤢" }
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
