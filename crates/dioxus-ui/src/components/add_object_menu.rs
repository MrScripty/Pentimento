//! Add Object popup menu component (Shift+A)

use dioxus::prelude::*;
use pentimento_ipc::PrimitiveType;

use crate::bridge::DioxusBridge;

const ADD_MENU_CSS: &str = r#"
.add-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 300;
}

.add-menu {
    position: absolute;
    min-width: 150px;
    background: rgba(30, 30, 30, 0.98);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 8px;
}

.menu-title {
    font-size: 11px;
    text-transform: uppercase;
    color: rgba(255, 255, 255, 0.5);
    margin: 0 0 8px 8px;
    letter-spacing: 0.05em;
}

.menu-items {
    display: flex;
    flex-direction: column;
}

.menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.9);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
}

.menu-item:hover {
    background: rgba(255, 255, 255, 0.1);
}

.menu-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.1);
    margin: 8px 0;
}
"#;

#[derive(Props, Clone, PartialEq)]
pub struct AddObjectMenuProps {
    pub show: bool,
    pub position: (f32, f32),
    pub bridge: DioxusBridge,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn AddObjectMenu(props: AddObjectMenuProps) -> Element {
    let primitives = [
        (PrimitiveType::Cube, "Cube"),
        (PrimitiveType::Sphere, "Sphere"),
        (PrimitiveType::Cylinder, "Cylinder"),
        (PrimitiveType::Plane, "Plane"),
        (PrimitiveType::Torus, "Torus"),
        (PrimitiveType::Cone, "Cone"),
        (PrimitiveType::Capsule, "Capsule"),
    ];

    let on_close = props.on_close.clone();
    let handle_backdrop_click = move |_| {
        on_close.call(());
    };

    rsx! {
        if props.show {
            style { {ADD_MENU_CSS} }
            div {
                class: "add-menu-backdrop",
                onclick: handle_backdrop_click,
                div {
                    class: "add-menu panel",
                    style: "left: {props.position.0}px; top: {props.position.1}px;",
                    onclick: move |e| e.stop_propagation(),
                    h3 { class: "menu-title", "Add Object" }
                    div { class: "menu-items",
                        for (prim_type, name) in primitives.iter() {
                            {
                                let bridge = props.bridge.clone();
                                let on_close = props.on_close.clone();
                                let prim = *prim_type;
                                rsx! {
                                    button {
                                        class: "menu-item",
                                        onclick: move |_| {
                                            bridge.add_object(prim, None, None);
                                            on_close.call(());
                                        },
                                        "{name}"
                                    }
                                }
                            }
                        }
                        div { class: "menu-divider" }
                        {
                            let bridge = props.bridge.clone();
                            let on_close = props.on_close.clone();
                            rsx! {
                                button {
                                    class: "menu-item",
                                    onclick: move |_| {
                                        bridge.add_paint_canvas(None, None);
                                        on_close.call(());
                                    },
                                    "Paint"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
