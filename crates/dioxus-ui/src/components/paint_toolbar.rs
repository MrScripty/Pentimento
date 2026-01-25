//! Paint toolbar component - shows brush tools when in paint mode

use dioxus::prelude::*;
use crate::bridge::DioxusBridge;

const PAINT_TOOLBAR_CSS: &str = r#"
.paint-toolbar {
    position: fixed;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 20px;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 24px;
    z-index: 200;
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
}

.paint-tool:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
}

.paint-tool-selected {
    background: rgba(100, 150, 255, 0.3);
    color: white;
}

.toolbar-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    margin-left: 8px;
}
"#;

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

    rsx! {
        style { {PAINT_TOOLBAR_CSS} }
        div { class: "paint-toolbar panel",
            button {
                class: "paint-tool paint-tool-selected",
                title: "Brush (B)",
                "B"  // Using letter as placeholder icon
            }
            span { class: "toolbar-hint", "Press Tab to exit" }
        }
    }
}
