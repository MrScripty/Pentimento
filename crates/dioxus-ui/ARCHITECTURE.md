# Dioxus UI Architecture: Native Rust UI with Bevy Integration

This document explains how the Dioxus UI integrates with Bevy through shared wgpu resources, the Blitz rendering pipeline, and the IPC bridge for UI/backend communication.

## Table of Contents

1. [Overview](#overview)
2. [Architecture Diagram](#architecture-diagram)
3. [Rendering Pipeline](#rendering-pipeline)
4. [Module Structure](#module-structure)
5. [IPC Bridge](#ipc-bridge)
6. [Input Handling](#input-handling)
7. [Texture Format & Color Space](#texture-format--color-space)
8. [Thread Safety](#thread-safety)
9. [Known Limitations](#known-limitations)

---

## Overview

Pentimento uses **Dioxus** for its native Rust UI, with **Blitz** handling DOM/CSS/layout and **Vello** for GPU-accelerated 2D rendering. The UI renders directly to a Bevy-owned GPU texture (zero-copy), which is then composited over the 3D scene.

### Key Technologies

| Component | Role |
|-----------|------|
| **Dioxus** | Reactive UI framework (RSX components, signals, event handlers) |
| **Blitz** | DOM tree, CSS styling, Taffy layout engine |
| **Vello** | GPU compute shader 2D renderer |
| **anyrender** | 2D drawing abstraction bridging Blitz to Vello |
| **Bevy** | 3D engine, window management, GPU context |

### Why This Architecture?

- **Native Rust UI**: No WebView/CEF overhead, single binary deployment
- **Zero-copy rendering**: Vello renders directly to Bevy's GPU texture
- **Shared GPU context**: Both Bevy and Vello use the same wgpu device
- **Full CSS support**: Blitz provides web-compatible styling and layout

---

## Architecture Diagram

```
+---------------------------------------------------------------------------+
|                              Bevy Main World                              |
+---------------------------------------------------------------------------+
|                                                                           |
|  +------------------+     +------------------+     +--------------------+ |
|  |  BlitzDocument   |     |  DioxusBridge    |     |  DioxusInputState  | |
|  |  (NonSend)       |<----|  (IPC channel)   |     |  (event queue)     | |
|  |                  |     +------------------+     +--------------------+ |
|  |  - VirtualDom    |                                       |            |
|  |  - BaseDocument  |<--------------------------------------+            |
|  |  - CSS/Layout    |         Mouse/Keyboard events                      |
|  +--------+---------+                                                    |
|           | paint_to_scene()                                             |
|           v                                                              |
|  +------------------+                                                    |
|  | VelloSceneBuffer | -----------------------+                           |
|  | (Scene graph)    |                        | Extracted to              |
|  +------------------+                        | render world              |
|                                              v                           |
+---------------------------------------------------------------------------+
|                              Bevy Render World                            |
+---------------------------------------------------------------------------+
|                                                                           |
|  +----------------------+    +----------------------------------------+  |
|  | SharedVelloRenderer  |    |           GpuImage (Rgba8Unorm)        |  |
|  | (Arc<Mutex<...>>)    |--->|     - STORAGE_BINDING for Vello        |  |
|  +----------------------+    |     - TEXTURE_BINDING for Bevy         |  |
|                              +----------------------------------------+  |
|                                               |                          |
|                                               v                          |
|                              +----------------------------------------+  |
|                              |         UiBlendMaterial                |  |
|                              |   - Samples texture                    |  |
|                              |   - Linear->sRGB conversion            |  |
|                              |   - Alpha compositing                  |  |
|                              +----------------------------------------+  |
|                                               |                          |
|                                               v                          |
|                              +----------------------------------------+  |
|                              |       Final Frame (over 3D scene)      |  |
|                              +----------------------------------------+  |
+---------------------------------------------------------------------------+
```

---

## Rendering Pipeline

### Frame Lifecycle

Each frame follows this pipeline:

1. **Input Processing** (`build_ui_scene`)
   - Drain queued mouse/keyboard events from the channel
   - Forward events to `BlitzDocument::handle_event()`

2. **VirtualDom Update** (`BlitzDocument::poll()`)
   - Dioxus processes pending state changes (signals)
   - DOM tree is updated with new/removed nodes

3. **Style & Layout Resolution** (`resolve()`)
   - Blitz computes CSS styles for all nodes
   - Taffy layout engine calculates positions/sizes

4. **Scene Building** (`paint_to_scene()`)
   - `blitz-paint` traverses the styled DOM tree
   - Generates `anyrender` draw commands
   - `anyrender_vello::VelloScenePainter` converts to Vello `Scene`

5. **Extraction** (Bevy extract phase)
   - `VelloSceneBuffer` is cloned to render world
   - `DioxusUiState` (dimensions) extracted

6. **GPU Rendering** (`render_vello_to_texture`)
   - Vello's compute shaders render the scene
   - Output goes directly to Bevy's `GpuImage` texture (zero-copy)

7. **Compositing** (Bevy's UI pass)
   - `UiBlendMaterial` samples the texture
   - Shader converts linear->sRGB, handles alpha
   - Blended over 3D scene

### Deferred Initialization

The UI texture and BlitzDocument are created in `deferred_setup_dioxus_texture`, which waits 2 frames after startup. This allows the window size to stabilize before allocating resources.

---

## Module Structure

### `crates/dioxus-ui/`

```
src/
+-- lib.rs              # Public exports, re-exports Vello/Blitz types
+-- app.rs              # PentimentoApp root component, global CSS
+-- document.rs         # BlitzDocument wrapper (DOM + layout + rendering)
+-- bridge.rs           # DioxusBridge IPC channel to Bevy
+-- renderer.rs         # VelloRenderer, SharedVelloRenderer
+-- state.rs            # UiState, RenderStats
+-- components/
    +-- mod.rs          # Component exports
    +-- toolbar.rs      # Top navigation bar, menus, tool buttons
    +-- side_panel.rs   # Right panel: properties, lighting, AO
    +-- add_object_menu.rs  # Shift+A popup menu
```

### `crates/app/src/render/`

```
ui_dioxus.rs            # DioxusRenderPlugin: Bevy integration
ui_blend_material.rs    # UiBlendMaterial for alpha compositing
shaders/
+-- ui_blend.wgsl       # Linear->sRGB conversion shader
```

### Key Structs

| Struct | Location | Description |
|--------|----------|-------------|
| `BlitzDocument` | `document.rs` | Wraps `DioxusDocument`, provides `poll()`, `paint_to_scene()`, `handle_event()` |
| `DioxusBridge` | `bridge.rs` | Cloneable sender half of IPC channel to Bevy |
| `DioxusBridgeHandle` | `bridge.rs` | Bevy-side receiver for UI messages |
| `SharedVelloRenderer` | `renderer.rs` | `Arc<Mutex<Renderer>>` for thread-safe render world usage |
| `DioxusRenderPlugin` | `ui_dioxus.rs` | Bevy plugin wiring everything together |

---

## IPC Bridge

The UI communicates with Bevy through typed Rust channels (no JSON serialization needed in native mode).

### Message Flow

```
+--------------------+                      +--------------------+
|    Dioxus UI       |                      |       Bevy         |
|                    |                      |                    |
|  DioxusBridge -----+---- UiToBevy ------->| DioxusBridgeHandle |
|                    |                      |                    |
|                    |<--- BevyToUi --------+---- (to_ui)        |
+--------------------+                      +--------------------+
```

### Message Types (`crates/ipc/src/lib.rs`)

**UI -> Bevy (`UiToBevy`)**:
- `CameraCommand` - Orbit, pan, zoom, reset
- `ObjectCommand` - Select, delete, duplicate
- `MaterialCommand` - Update properties
- `AddObject` - Spawn primitive meshes
- `UpdateLighting` - Sun direction, intensity, time-of-day
- `UpdateAmbientOcclusion` - SSAO settings
- `UiDirty` - Signal that UI needs re-render

**Bevy -> UI (`BevyToUi`)**:
- `Initialize` - Scene state on startup
- `SelectionChanged` - Object selection updates
- `RenderStats` - FPS, frame time
- `DiffusionProgress/Complete` - Texture generation status

### Usage in Components

```rust
// In a Dioxus component:
let bridge = props.bridge.clone();
let handle_reset = move |_| {
    bridge.camera_reset();  // Sends CameraCommand::Reset
};

rsx! {
    button { onclick: handle_reset, "Reset Camera" }
}
```

---

## Input Handling

### Event Flow

1. Bevy captures raw input events (mouse, keyboard)
2. Events are converted to Blitz's `UiEvent` types
3. Sent through `DioxusEventSender` channel
4. `build_ui_scene` drains the channel
5. `BlitzDocument::handle_event()` dispatches to DOM

### Click Tolerance

To prevent small mouse jitter from triggering drag detection (which would prevent click synthesis), the input handler implements click tolerance:

```rust
const CLICK_TOLERANCE: f32 = 8.0; // pixels

// In PointerMove: only report buttons pressed if moved beyond tolerance
let buttons = if dx > CLICK_TOLERANCE || dy > CLICK_TOLERANCE {
    self.buttons_pressed
} else {
    MouseEventButtons::empty()
};
```

### Hit Testing

The `BlitzDocument` includes custom hit testing for debugging:

```rust
// Custom recursive hit testing (depth-first, returns deepest element)
fn deepest_hit(&self, doc: &BaseDocument, x: f32, y: f32) -> Option<usize>

// Blitz's built-in hit testing (for comparison)
doc.hit(x, y) -> Option<HitResult>
```

Both are logged on clicks for debugging coordinate/layout issues.

---

## Texture Format & Color Space

### Why `Rgba8Unorm` (Linear)?

Vello's compute shaders require `STORAGE_BINDING` on the output texture:

| Format | STORAGE_BINDING | Color Space |
|--------|-----------------|-------------|
| `Rgba8Unorm` | Supported | Linear |
| `Rgba8UnormSrgb` | Not supported | sRGB |

### Compositing Shader

The `ui_blend.wgsl` shader handles:

1. **Color space conversion**: Linear -> sRGB via gamma curve
2. **Alpha blending**: Straight alpha (not premultiplied)

```wgsl
@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(ui_texture, ui_sampler, in.uv);

    // Linear -> sRGB
    let srgb = pow(color.rgb, vec3(1.0 / 2.2));

    // Output straight alpha for ALPHA_BLENDING
    return vec4(srgb, color.a);
}
```

### Comparison with Svelte UI

| Aspect | Svelte (WebKit) | Dioxus (Vello) |
|--------|-----------------|----------------|
| Renderer | WebKit browser | Vello GPU compute |
| Texture format | `Rgba8UnormSrgb` | `Rgba8Unorm` |
| Data transfer | CPU -> GPU upload | Zero-copy GPU |
| Color space | sRGB (native) | Linear (converted in shader) |
| Compositing | Default `ImageNode` | Custom `UiMaterial` |

---

## Thread Safety

### The Challenge

- Bevy's render world runs on a separate thread
- Dioxus `VirtualDom` is `!Send` (not thread-safe)
- Vello's `Renderer` is `!Send + !Sync`

### Solution

1. **Main World**: `BlitzDocument` stays as `NonSend` resource
   - Only accessed from main thread
   - Exclusive systems (`fn(world: &mut World)`) required

2. **Scene Extraction**: `VelloSceneBuffer` is `Clone + Send`
   - Scene graph cloned to render world each frame
   - No runtime cost (Vello scenes are cheap to clone)

3. **Render World**: `SharedVelloRenderer` wraps in `Arc<Mutex<...>>`
   - Mutex only held during single `render_to_texture` call
   - Minimal contention (once per frame)

```rust
pub struct SharedVelloRenderer(Arc<Mutex<Renderer>>);

impl SharedVelloRenderer {
    pub fn render_to_texture(...) {
        self.0.lock().unwrap().render_to_texture(...)
    }
}
```

---

## Known Limitations

### Blitz CSS Limitations

1. **`pointer-events: none` not respected**
   - Blitz ignores this CSS property
   - Workaround: Don't use invisible overlay divs for keyboard capture

2. **`position: fixed` hit testing issues**
   - Fixed-position elements may not hit-test correctly
   - Workaround: Use normal flow layout where possible

3. **No stacking context from root**
   - CSS `z-index` may not work as expected on deeply nested fixed elements
   - Workaround: Keep positioned elements close to root

### Current Workarounds

The toolbar uses normal flow layout instead of `position: fixed`:

```css
/* Instead of position: fixed for hit testing compatibility */
.toolbar {
    width: 100%;
    height: 48px;
    display: flex;
    /* ... */
}
```

### Future Improvements

- [ ] Keyboard input handling (currently removed due to `pointer-events` issue)
- [ ] Text rendering improvements via Vello's text shaping
- [ ] Custom fonts
- [ ] Performance: dirty-region tracking to skip unchanged areas

---

## Implementation Files

| File | Description |
|------|-------------|
| `crates/dioxus-ui/src/lib.rs` | Public API, re-exports |
| `crates/dioxus-ui/src/document.rs` | BlitzDocument, hit testing, scene painting |
| `crates/dioxus-ui/src/app.rs` | Root component, global CSS |
| `crates/dioxus-ui/src/bridge.rs` | IPC channel (DioxusBridge) |
| `crates/dioxus-ui/src/renderer.rs` | VelloRenderer, SharedVelloRenderer |
| `crates/dioxus-ui/src/components/*.rs` | UI components (Toolbar, SidePanel, etc.) |
| `crates/app/src/render/ui_dioxus.rs` | Bevy plugin, systems, resources |
| `crates/app/src/render/ui_blend_material.rs` | Custom UiMaterial |
| `crates/app/src/render/shaders/ui_blend.wgsl` | Compositing shader |
| `crates/ipc/src/lib.rs` | Message types for UI<->Bevy communication |
