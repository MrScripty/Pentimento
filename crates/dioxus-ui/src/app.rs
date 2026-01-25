//! Main Pentimento application component

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;
use crate::components::{AddObjectMenu, SidePanel, Toolbar};
use crate::state::RenderStats;

const APP_CSS: &str = r#"
* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

html {
    width: 100%;
    height: 100%;
    /* Force stacking context on root for position:fixed hit testing */
    position: relative;
    z-index: 0;
}

body {
    width: 100%;
    height: 100%;
}

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: transparent;
    color: white;
    overflow: hidden;
}

main {
    width: 100%;
    height: 100%;
    /* Create a stacking context with low z-index so fixed children are above */
    position: relative;
    z-index: 0;
}

/* Re-enable pointer events for actual interactive elements */
main > * {
    pointer-events: auto;
}

.panel {
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
}
"#;

#[derive(Props, Clone, PartialEq)]
pub struct PentimentoAppProps {
    pub bridge: DioxusBridge,
}

/// Main Pentimento UI application
#[component]
pub fn PentimentoApp(props: PentimentoAppProps) -> Element {
    // Reactive state
    let render_stats = use_signal(|| RenderStats::default());
    let selected_objects = use_signal(|| Vec::<String>::new());

    // Add object menu state
    let mut show_add_menu = use_signal(|| false);
    let mut add_menu_position = use_signal(|| (200.0f32, 200.0f32));

    // Handle keyboard events for Shift+A
    let handle_keydown = move |evt: Event<KeyboardData>| {
        if evt.data().modifiers().shift() && evt.data().key() == Key::Character("A".to_string()) {
            show_add_menu.set(true);
        }
    };

    rsx! {
        // Note: Keyboard capture removed - Blitz doesn't respect pointer-events:none
        // TODO: Add keyboard handling to individual components or find alternative
        style { {APP_CSS} }
        Toolbar {
            render_stats: render_stats(),
            bridge: props.bridge.clone()
        }
        SidePanel {
            selected_objects: selected_objects(),
            bridge: props.bridge.clone()
        }
        AddObjectMenu {
            show: show_add_menu(),
            position: add_menu_position(),
            bridge: props.bridge.clone(),
            on_close: move |_| show_add_menu.set(false)
        }
    }
}
