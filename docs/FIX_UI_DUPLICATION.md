# Fix: UI Duplication and Slowness

**Date:** 2026-01-26
**Issue:** UI is duplicated (entire UI appears twice - top half and bottom half) after mouse movement. Hotkeys were also broken by a previous fix attempt.

---

## Problem Analysis

### Symptoms
- UI starts correctly (single instance)
- After mouse movement, UI doubles (one in top half, one in bottom half of window)
- 3D viewport is NOT doubled - only the Dioxus UI
- Previous fix attempt (removing force_render) broke hotkeys

### Root Cause: `vdom.rebuild()` APPENDS to Root

In dioxus-core-0.7.1, the `rebuild()` method ends with:
```rust
// In dioxus-core/src/virtual_dom.rs
pub fn rebuild(&mut self, to: &mut impl WriteMutations) {
    // ...creates new nodes...
    let m = self.create_scope(Some(to), ScopeId::ROOT, new_nodes, None);
    to.append_children(ElementId(0), m);  // <-- BUG: APPENDS, doesn't replace!
}
```

The Dioxus documentation comment says: "Rebuilding implies we append the created elements to the root"

**Design Intent:**
- `rebuild()` is meant for generating mutations to populate an **empty** DOM from scratch
- It's designed to be called ONCE at initialization
- `render_immediate()` is for subsequent incremental updates (diff-based)

**Bug in Pentimento:**
The current `force_render()` calls `initial_build()` which calls `vdom.rebuild()`. When called repeatedly (on every mouse event), it appends a new complete copy of the UI to the root each time.

---

## Research: Dioxus 0.7 Update Mechanisms

### VirtualDom Methods

| Method | Purpose | Safe to call repeatedly? |
|--------|---------|-------------------------|
| `rebuild()` | Initial DOM population | NO - appends to root |
| `render_immediate()` | Incremental diff updates | YES - updates in place |
| `mark_dirty()` | Mark scope for re-render | YES |
| `poll()` / `wait_for_work()` | Process pending updates | YES |

### dioxus-native-dom Implementation

**`initial_build()`** (in dioxus-native-dom-0.7.3):
```rust
pub fn initial_build(&mut self) {
    let mut writer = MutationWriter::new(&mut self.inner, &mut self.vdom_state);
    self.vdom.rebuild(&mut writer);  // Calls rebuild - APPENDS!
}
```

**`poll()`** (in dioxus-native-dom-0.7.3):
```rust
fn poll(&mut self, cx: Option<TaskContext>) -> bool {
    // Check if there's work via wait_for_work future
    let fut = self.vdom.wait_for_work();
    pin_mut!(fut);
    match fut.poll_unpin(&mut cx) {
        std::task::Poll::Ready(_) => {}
        std::task::Poll::Pending => return false,  // Early exit!
    }
    // Only reaches here if there's work
    let mut writer = MutationWriter::new(&mut self.inner, &mut self.vdom_state);
    self.vdom.render_immediate(&mut writer);  // Incremental update
    true
}
```

### Why `mark_dirty() + poll()` Was Changed

The commit 89348eb changed `force_render()` because the comment claimed `poll()` doesn't process dirty scopes. However, tracing the code shows it SHOULD work:

1. `mark_dirty(ScopeId::ROOT)` → `dirty_scopes.insert()`
2. `poll()` → `wait_for_work()` → `has_dirty_scopes()` returns true
3. `wait_for_work()` returns Ready
4. `poll()` calls `render_immediate()` which processes dirty scopes

The issue may have been timing, borrow conflicts, or edge cases. The "fix" of using `initial_build()` made hotkeys work but introduced the duplication bug.

---

## Why Blitz Instead of Dioxus Desktop

Pentimento uses Blitz+Vello instead of Dioxus Desktop for these reasons:

1. **Zero-copy GPU rendering**: Vello renders directly to Bevy's GPU texture
2. **Shared wgpu context**: Same GPU device as Bevy, no context switching
3. **No WebView overhead**: Dioxus Desktop uses a WebView (browser), requiring separate process/GPU context and CPU→GPU texture uploads
4. **Single binary**: Pure Rust, no CEF/WebView dependencies

**Trade-offs:**
- Blitz is less mature than browser rendering
- Manual VirtualDom management (`poll()`, `force_render()`) required
- Hit-testing issues with positioned elements
- No native `<input>` elements

---

## Reference: Vanilla Dioxus 0.7 Pattern

From the working reference at `/media/jeremy/OrangeCream/Linux Software/dioxus-floating/`:

```rust
// Vanilla Dioxus Desktop - VirtualDom handled automatically
fn main() {
    dioxus::launch(App);  // Runtime handles everything
}

#[component]
fn App() -> Element {
    let mut show_menu = use_signal(|| false);

    rsx! {
        div {
            onkeydown: move |e| {
                if e.modifiers().shift() && e.key() == Key::Character("A".to_string()) {
                    show_menu.set(true);  // Direct signal update - auto re-renders
                }
            },
            // ...
        }
    }
}
```

In Pentimento, manual intervention is needed because Blitz doesn't have the same mature event handling as Dioxus Desktop's WebView.

---

## Fix Plan

### Step 1: Fix `force_render()` to use `render_immediate()` directly

**File:** `crates/dioxus-ui/src/document.rs`

Change from:
```rust
pub fn force_render(&mut self) {
    self.doc.initial_build();  // BUG: appends duplicate DOM!
    self.doc.inner.borrow_mut().resolve(0.0);
}
```

To:
```rust
pub fn force_render(&mut self) {
    // Mark root scope dirty to ensure component re-runs
    self.doc.vdom.mark_dirty(dioxus_core::ScopeId::ROOT);

    // Call render_immediate directly (bypasses wait_for_work check in poll)
    let mut writer = dioxus_native_dom::MutationWriter::new(
        &mut self.doc.inner,
        &mut self.doc.vdom_state
    );
    self.doc.vdom.render_immediate(&mut writer);

    // Re-resolve layout
    self.doc.inner.borrow_mut().resolve(0.0);
}
```

This approach:
- Marks the root scope dirty so Dioxus knows to re-run the component
- Calls `render_immediate()` directly instead of going through poll's wait_for_work
- Uses incremental diffing (doesn't append new nodes)

### Step 2: Restore force_render call after IPC messages

**File:** `crates/app/src/render/ui_dioxus.rs`

Restore the force_render call that was removed. IPC messages update SharedUiState, and the component needs to re-render to read that state.

### Step 3: Keep logging cleanup (already done)

The logging changes are correct and improve performance.

---

## Files to Modify

1. `crates/dioxus-ui/src/document.rs` - Fix force_render implementation
2. `crates/app/src/render/ui_dioxus.rs` - Restore force_render after IPC

---

## Why This Fix Should Work

- `render_immediate()` performs incremental DOM updates (diff-based)
- Unlike `rebuild()`, it doesn't append new nodes to the root
- `mark_dirty(ROOT)` ensures the component function re-executes
- Bypasses `poll()`'s early-exit check that may have caused hotkey issues

---

## Verification Steps

1. Run the application
2. Move the mouse - verify UI does NOT double
3. Press Shift+A - verify Add Object menu opens
4. Use sliders - verify they respond correctly
5. Verify overall UI responsiveness

---

## Rollback

If the fix doesn't work, the changes can be reverted by:
1. Restoring `initial_build()` in `force_render()`
2. The original `force_render()` call location in `handle_ui_to_bevy_messages` (if changed)
