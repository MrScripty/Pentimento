//! Main Pentimento application component

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;
use crate::components::{SidePanel, Toolbar};
use crate::state::RenderStats;

const APP_CSS: &str = r#"
* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: transparent;
    color: white;
    overflow: hidden;
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

    rsx! {
        style { {APP_CSS} }
        Toolbar {
            render_stats: render_stats(),
            bridge: props.bridge.clone()
        }
        SidePanel {
            selected_objects: selected_objects(),
            bridge: props.bridge.clone()
        }
    }
}
