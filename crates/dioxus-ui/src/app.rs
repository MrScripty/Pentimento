//! Main Pentimento application component

use dioxus::prelude::*;

use pentimento_ipc::{BevyToUi, EditMode};

use crate::bridge::DioxusBridge;
use crate::components::{AddObjectMenu, PaintSidePanel, PaintToolbar, SidePanel, Toolbar};
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

main, .app-root {
    width: 100%;
    height: 100%;
    /* Flexbox layout - Blitz hit testing works with normal flow */
    display: flex;
    flex-direction: column;
    position: relative;
    z-index: 0;
}

/* Re-enable pointer events for actual interactive elements */
main > *, .app-root > * {
    pointer-events: auto;
}

/* Main content area below toolbar - fills remaining space */
.main-content {
    flex: 1;
    display: flex;
    flex-direction: row;
    pointer-events: none;
    position: relative;  /* Anchor for absolute-positioned keyboard-focus-trap */
}

/* Spacer pushes side panel to the right, allows click-through */
.content-spacer {
    flex: 1;
    pointer-events: none;
}

/* Focus trap for viewport area - clicking here keeps focus for keyboard events.
   Inside main-content (position: relative parent).
   Uses position: absolute which works with Blitz hit-testing when parent is relative. */
.keyboard-focus-trap {
    position: absolute;
    inset: 0;
    z-index: 0;  /* Behind sibling UI elements */
    pointer-events: auto;
    outline: none;
    background: transparent;
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
    // DEBUG: Direct stderr to bypass tracing - confirm component actually runs
    eprintln!(">>> PentimentoApp component rendering <<<");
    tracing::info!("PentimentoApp component rendering");

    // Reactive state
    let render_stats = use_signal(|| RenderStats::default());
    let selected_objects = use_signal(|| Vec::<String>::new());

    // Add object menu state - uses Dioxus signals for reactivity
    // Signals trigger re-renders when changed (unlike SharedUiState which doesn't)
    let mut show_add_menu = use_signal(|| false);
    let mut add_menu_position = use_signal(|| (0.0f32, 0.0f32));

    // Cursor position tracking for menu positioning
    let mut cursor_pos = use_signal(|| (0.0f32, 0.0f32));

    // Edit mode state
    let mut edit_mode = use_signal(|| EditMode::None);

    // Toolbar menu state - lifted from Toolbar for cross-component control
    let mut open_menu = use_signal(|| None::<String>);

    // Process messages from channel - update SIGNALS which trigger reactivity
    while let Some(msg) = props.bridge.try_recv_from_bevy() {
        tracing::info!("Component received message from Bevy: {:?}", msg);
        match msg {
            BevyToUi::EditModeChanged { mode } => {
                tracing::info!("Setting edit_mode to {:?}", mode);
                edit_mode.set(mode);
            }
            BevyToUi::ShowAddObjectMenu { show, position } => {
                eprintln!(">>> IPC: ShowAddObjectMenu show={} <<<", show);
                tracing::info!("IPC: Setting show_add_menu signal from {} to {}", show_add_menu(), show);
                show_add_menu.set(show);
                tracing::info!("IPC: After set, show_add_menu() = {}", show_add_menu());
                if let Some([x, y]) = position {
                    add_menu_position.set((x, y));
                    tracing::info!("IPC: Set position to ({}, {})", x, y);
                }
            }
            BevyToUi::CloseMenus => {
                // Close toolbar menus when clicking outside UI (e.g., in viewport)
                open_menu.set(None);
            }
            _ => {
                // Other messages not yet handled
            }
        }
    }

    // Debug: log the signal values that will be passed to AddObjectMenu
    tracing::info!("Rendering with show_add_menu={}, position=({:.0}, {:.0})",
        show_add_menu(), add_menu_position().0, add_menu_position().1);

    // Handle keyboard events for Shift+A, ESC, and Ctrl+Z
    let bridge_for_keydown = props.bridge.clone();
    let handle_keydown = move |evt: Event<KeyboardData>| {
        let key = evt.data().key();
        let mods = evt.data().modifiers();

        // Debug: log keyboard events
        tracing::info!("Dioxus keydown: key={:?} shift={} ctrl={}", key, mods.shift(), mods.ctrl());

        // ESC closes add object menu
        if matches!(&key, Key::Escape) && show_add_menu() {
            show_add_menu.set(false);
            return;
        }

        // Shift+A opens Add Object popup menu at cursor position
        let is_a = matches!(&key, Key::Character(c) if c == "a" || c == "A");
        if mods.shift() && !mods.ctrl() && is_a {
            tracing::info!("Dioxus: Shift+A detected, opening Add Object menu at {:?}", cursor_pos());
            add_menu_position.set(cursor_pos());
            show_add_menu.set(true);
        }

        // Ctrl+Z for undo (check both cases)
        let is_z = matches!(&key, Key::Character(c) if c == "z" || c == "Z");
        if mods.ctrl() && !mods.shift() && is_z {
            tracing::info!("Dioxus: Ctrl+Z detected, performing undo");
            bridge_for_keydown.paint_undo();
        }
    };

    // Handle mouse move to track cursor position
    let handle_mousemove = move |evt: Event<MouseData>| {
        let coords = evt.client_coordinates();
        cursor_pos.set((coords.x as f32, coords.y as f32));
    };

    rsx! {
        style { {APP_CSS} }

        // Root wrapper for UI elements that need click-through behavior
        // pointer-events: none lets clicks pass through to 3D scene
        // tabindex: 0 makes element focusable so it can receive keyboard events
        // onkeydown on root because Blitz dispatches keyboard events to focused/root element
        div {
            class: "app-root",
            style: "outline: none; pointer-events: none;",
            tabindex: 0,
            onkeydown: handle_keydown,
            onmousemove: handle_mousemove,
            Toolbar {
                render_stats: render_stats(),
                bridge: props.bridge.clone(),
                open_menu: open_menu,
                shared_state: props.bridge.get_shared_state(),
            }
            AddObjectMenu {
                show: show_add_menu(),
                position: add_menu_position(),
                bridge: props.bridge.clone(),
                on_close: move |_| {
                    show_add_menu.set(false);
                }
            }
            PaintToolbar {
                visible: edit_mode() == EditMode::Paint,
                bridge: props.bridge.clone()
            }
            // Main content area - flexbox row with spacer and side panel
            // Uses normal flow layout so Blitz hit testing works
            // Has position: relative to anchor the keyboard-focus-trap
            div {
                class: "main-content",
                // Focus trap for viewport area - clicking here maintains keyboard focus.
                // tabindex: 0 makes it focusable so keyboard events continue to work.
                // Uses position: absolute which works with Blitz hit-testing.
                div {
                    class: "keyboard-focus-trap",
                    tabindex: 0,
                    onclick: move |_| open_menu.set(None),
                }
                // Spacer fills remaining space, pushes side panel to right
                div { class: "content-spacer" }
                // Side panel in normal flow (no position:absolute)
                if edit_mode() == EditMode::Paint {
                    PaintSidePanel {
                        bridge: props.bridge.clone()
                    }
                } else {
                    SidePanel {
                        selected_objects: selected_objects(),
                        bridge: props.bridge.clone(),
                    }
                }
            }
        }
    }
}
