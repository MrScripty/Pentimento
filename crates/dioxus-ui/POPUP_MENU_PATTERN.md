# Popup Menu Pattern in Blitz/Dioxus

This document explains how to implement keyboard-triggered popup menus (like Shift+A) in a Blitz-rendered Dioxus application.

## Key Differences from Browser-Based Dioxus

Blitz is a **headless native renderer** for Dioxus - it doesn't use a browser/WebView. This means:

1. **Manual focus management** - Blitz doesn't auto-focus elements on click like browsers do
2. **Keyboard events go to focused element** - If nothing is focused, events go to `<html>` root
3. **No automatic reactivity on external state changes** - Must call `rebuild()` to force component re-execution

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│  app-root (tabindex=0, onkeydown, onmousemove)              │
│  ├── Toolbar                                                │
│  ├── AddObjectMenu (position: absolute)                     │
│  └── main-content                                           │
│      ├── keyboard-focus-trap (tabindex=0, position:absolute)│
│      ├── content-spacer                                     │
│      └── SidePanel                                          │
└─────────────────────────────────────────────────────────────┘
```

## Required Components

### 1. Focus Management

Blitz dispatches keyboard events to the **focused element**. Without focus, keyboard handlers won't fire.

```rust
// In document.rs - set initial focus after building the document
fn find_first_focusable(doc: &BaseDocument) -> Option<usize> {
    // Recursively find first element with tabindex or form element
}

// After initial_build():
if let Some(focusable_id) = Self::find_first_focusable(&inner) {
    inner.set_focus_to(focusable_id);
}
```

**Key insight**: Add `tabindex: 0` to any element that should receive focus when clicked:

```rust
div {
    class: "app-root",
    tabindex: 0,           // Makes element focusable
    onkeydown: handler,    // Receives keyboard events when focused
}
```

### 2. Cursor Position Tracking

Track cursor position on the **root element** so it's updated regardless of where the mouse is:

```rust
// State
let mut cursor_pos = use_signal(|| (0.0f32, 0.0f32));

// Handler
let handle_mousemove = move |evt: Event<MouseData>| {
    let coords = evt.client_coordinates();
    cursor_pos.set((coords.x as f32, coords.y as f32));
};

// Attach to root element (not a child)
div {
    class: "app-root",
    onmousemove: handle_mousemove,  // Track cursor everywhere
    // ...
}
```

### 3. Keyboard Handler

```rust
let handle_keydown = move |evt: Event<KeyboardData>| {
    let key = evt.data().key();
    let mods = evt.data().modifiers();

    // Shift+A opens menu at cursor
    let is_a = matches!(&key, Key::Character(c) if c == "a" || c == "A");
    if mods.shift() && !mods.ctrl() && is_a {
        add_menu_position.set(cursor_pos());
        show_add_menu.set(true);
    }

    // ESC closes menu
    if matches!(&key, Key::Escape) && show_add_menu() {
        show_add_menu.set(false);
    }
};
```

### 4. Focus Trap for Click Retention

When clicking in the viewport area, focus must transfer to a focusable element to keep keyboard events working:

```rust
div {
    class: "keyboard-focus-trap",
    tabindex: 0,  // Clicking here gives it focus, keyboard events still work
    onclick: move |_| { /* close other menus */ },
}
```

CSS for the focus trap:
```css
.keyboard-focus-trap {
    position: absolute;
    inset: 0;
    z-index: 0;
    pointer-events: auto;
    outline: none;
    background: transparent;
}
```

### 5. Popup Menu Component

```rust
#[component]
fn AddObjectMenu(props: AddObjectMenuProps) -> Element {
    // Early return if not shown
    if !props.show {
        return rsx! {};
    }

    rsx! {
        // Overlay fills parent (position: relative)
        div {
            class: "add-menu-overlay",
            // Backdrop catches clicks outside menu
            div {
                class: "add-menu-backdrop",
                onclick: move |_| props.on_close.call(()),
            }
            // Menu positioned at cursor
            div {
                class: "add-menu",
                style: "left: {props.position.0}px; top: {props.position.1}px;",
                onclick: move |e| e.stop_propagation(),  // Don't close on menu click
                // Menu items...
            }
        }
    }
}
```

CSS for positioning:
```css
.add-menu-overlay {
    position: absolute;
    inset: 0;
    z-index: 300;
    pointer-events: auto;
}

.add-menu-backdrop {
    position: absolute;
    inset: 0;
}

.add-menu {
    position: absolute;  /* Positioned by inline style */
    min-width: 150px;
    /* styling... */
}
```

## Force Render for External State Changes

If state changes come from outside Dioxus (e.g., IPC from Bevy), you must force a rebuild:

```rust
// In BlitzDocument
pub fn force_render(&mut self) {
    // initial_build() calls vdom.rebuild() which forces component execution
    self.doc.initial_build();
    self.doc.inner.borrow_mut().resolve(0.0);
}
```

This is necessary because `poll()` only processes async work - it doesn't re-run components unless they have pending state changes detected by Dioxus's reactivity system.

## Common Pitfalls

1. **Menu appears at (0, 0)** - Cursor tracking not on root element, or mouse hasn't moved yet
2. **Keyboard stops working after click** - Clicked element isn't focusable (missing `tabindex`)
3. **Menu doesn't appear from IPC** - Need to call `force_render()` / `rebuild()` after sending message
4. **Events don't reach handler** - Focus is on wrong element; Blitz sends events to focused element only

## Testing Checklist

- [ ] Move mouse, press Shift+A → menu appears at cursor
- [ ] Press Escape → menu closes
- [ ] Click backdrop → menu closes
- [ ] Click in viewport, press Shift+A → still works (focus retained)
- [ ] Click menu item → action fires, menu closes
