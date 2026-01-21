# Svelte + Bevy Single Window Compositing Demo

## Goal
Create a standalone desktop app demonstrating:
- Svelte 5 complex UI (rich editor, drag-drop, node graphs)
- Bevy 3D with advanced rendering (PBR, post-processing)
- Real-time diffusion texture streaming (local GPU + remote server)
- **True single native window** (no WASM, no stacked windows)

## Recommended Approach: Native Bevy + Offscreen Webview Compositing

Bevy owns the native window and render pipeline. Svelte runs in an **offscreen webview** (wry). The UI framebuffer is captured **on-demand** (only when UI changes) and composited as a texture in Bevy's render graph.

### Why This Approach
- **Native GPU performance** - Bevy runs natively, not in WASM
- **True single window** - Bevy owns the window, no surface conflicts
- **Complex Svelte UI** - Full web ecosystem (Tailwind, xyflow, etc.)
- **On-demand capture** - UI texture only updates when Svelte state changes (~5-20fps during interaction, 0 when idle)
- **Direct diffusion streaming** - Same GPU = direct texture handles, remote = network → upload

### Architecture
```
┌─────────────────────────────────────────────────────────┐
│                 Bevy Native Window                      │
│  ┌───────────────────────────────────────────────────┐  │
│  │              Bevy Render Pipeline                 │  │
│  │                                                   │  │
│  │   [3D Scene] ──► [Post-Processing] ──► [UI Comp] │  │
│  │        ▲                                    ▲     │  │
│  │        │                                    │     │  │
│  │   Diffusion                          UI Texture   │  │
│  │   Textures                           (cached)     │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
                                                 ▲
                              only updates when  │
                              Svelte state changes
                                                 │
                    ┌────────────────────────────┴───────┐
                    │     Offscreen Webview (wry)        │
                    │     └─ Svelte UI (complex)         │
                    │        - Rich editor               │
                    │        - Drag-drop                 │
                    │        - Node graphs (xyflow)      │
                    └────────────────────────────────────┘

Communication:
  [Bevy] ◄──IPC──► [Webview/Svelte]
    │
    ├── Input forwarding (mouse, keyboard)
    ├── Commands (scene control, settings)
    ├── Events (render stats, model loaded)
    └── Dirty flag + framebuffer capture
```

---

## Project Structure

```
bevy-svelte-demo/
├── Cargo.toml                 # Workspace root
├── package.json               # Svelte UI build (outputs to dist/)
├── vite.config.ts
│
├── ui/                        # Svelte frontend (runs in offscreen webview)
│   ├── index.html
│   ├── src/
│   │   ├── App.svelte        # Root UI component
│   │   ├── lib/
│   │   │   ├── bridge.ts     # IPC with Bevy host
│   │   │   ├── components/
│   │   │   │   ├── Toolbar.svelte
│   │   │   │   ├── SidePanel.svelte
│   │   │   │   ├── NodeGraph.svelte    # xyflow integration
│   │   │   │   ├── MaterialEditor.svelte
│   │   │   │   └── DiffusionPanel.svelte
│   │   │   └── stores/
│   │   │       └── scene.svelte.ts
│   │   └── styles/
│   │       └── global.css    # Transparent background
│   └── dist/                  # Built output (loaded by wry)
│
├── crates/
│   ├── app/                   # Main Bevy application
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs       # Entry point
│   │       ├── app.rs        # Bevy App setup
│   │       ├── render/
│   │       │   ├── mod.rs
│   │       │   └── ui_composite.rs  # Custom render node
│   │       ├── scene/
│   │       │   ├── camera.rs
│   │       │   ├── lighting.rs
│   │       │   └── setup.rs
│   │       ├── materials/
│   │       │   └── pbr.rs
│   │       └── input/
│   │           └── routing.rs  # Hit-test + forwarding
│   │
│   ├── webview/               # Offscreen webview management
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── offscreen.rs  # wry headless webview
│   │       ├── capture.rs    # Framebuffer capture
│   │       └── ipc.rs        # Message passing
│   │
│   └── diffusion/             # Texture streaming
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── local.rs      # candle/burn GPU inference
│           ├── remote.rs     # WebSocket client
│           └── texture.rs    # GPU upload management
```

---

## Key Implementation Details

### 1. Bevy App with Custom Render Node
```toml
# crates/app/Cargo.toml
[dependencies]
bevy = { version = "0.15", features = ["wayland"] }  # or x11
wry = "0.47"  # Offscreen webview
image = "0.25"  # Framebuffer handling
tokio = { version = "1", features = ["sync"] }
```

```rust
// Custom render node that composites UI texture over 3D scene
pub struct UiCompositeNode {
    ui_texture: Handle<Image>,
}

impl render_graph::Node for UiCompositeNode {
    fn run(&self, ...) {
        // Bind UI texture (RGBA with alpha)
        // Draw fullscreen quad with alpha blending
        // UI appears over 3D scene
    }
}
```

### 2. Offscreen Webview (wry)
```rust
// crates/webview/src/offscreen.rs
use wry::{WebViewBuilder, WebContext};

pub struct OffscreenWebview {
    webview: WebView,
    dirty: Arc<AtomicBool>,
}

impl OffscreenWebview {
    pub fn new(html_path: &Path, size: (u32, u32)) -> Self {
        // Create headless webview
        // Load Svelte app from dist/
        // Set up IPC handler
    }

    pub fn capture_if_dirty(&mut self) -> Option<Vec<u8>> {
        if self.dirty.swap(false, Ordering::SeqCst) {
            Some(self.webview.screenshot()) // Platform-specific
        } else {
            None
        }
    }
}
```

### 3. Input Routing
```rust
// crates/app/src/input/routing.rs
fn route_input(
    mouse_pos: Vec2,
    ui_regions: &[Rect],  // Known UI panel positions
    webview: &mut OffscreenWebview,
) -> InputTarget {
    for region in ui_regions {
        if region.contains(mouse_pos) {
            // Forward to webview
            webview.send_mouse_event(mouse_pos, ...);
            return InputTarget::Ui;
        }
    }
    InputTarget::Scene  // Let Bevy handle it
}
```

### 4. Diffusion Texture Streaming
```rust
// crates/diffusion/src/texture.rs
pub enum DiffusionSource {
    Local(candle_core::Device),   // Same GPU
    Remote(WebSocketClient),       // Network
}

impl DiffusionSource {
    pub async fn stream_to_texture(
        &mut self,
        texture: &mut Handle<Image>,
    ) {
        match self {
            Local(device) => {
                // Direct GPU tensor → Bevy texture
                // Zero-copy if same wgpu device
            }
            Remote(ws) => {
                // Receive bytes → decode → upload
                let bytes = ws.recv().await;
                texture.update(bytes);
            }
        }
    }
}
```

### 5. Svelte UI (Transparent Background)
```css
/* ui/src/styles/global.css */
:root {
    background: transparent;
}
body {
    background: transparent;
}
/* UI elements have their own backgrounds */
.panel {
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
}
```

### 6. IPC Bridge
```typescript
// ui/src/lib/bridge.ts
declare global {
    interface Window {
        __BEVY_IPC__: {
            postMessage: (msg: string) => void;
            onMessage: (callback: (msg: string) => void) => void;
        };
    }
}

export function sendCommand(cmd: object) {
    window.__BEVY_IPC__.postMessage(JSON.stringify(cmd));
}

export function markDirty() {
    sendCommand({ type: 'UiDirty' });
}

// Call markDirty() after any state change that affects rendering
```

---

## Implementation Steps

### Phase 1: Project Scaffolding
- [ ] Create Cargo workspace with crates/app, crates/webview, crates/diffusion
- [ ] Set up Vite + Svelte 5 + Tailwind in ui/ directory
- [ ] Configure build scripts (cargo + npm)

### Phase 2: Basic Bevy Window
- [ ] Minimal Bevy app with window and camera
- [ ] Add PBR scene with test objects
- [ ] Verify native rendering works

### Phase 3: Offscreen Webview
- [ ] Create wry offscreen webview wrapper
- [ ] Load simple HTML and capture framebuffer
- [ ] Test on Linux (WebKitGTK) - may need platform-specific code

### Phase 4: UI Compositing
- [ ] Create custom Bevy render node for UI overlay
- [ ] Upload captured framebuffer as texture
- [ ] Alpha-blend over 3D scene
- [ ] Verify transparency works

### Phase 5: Input Routing
- [ ] Define UI region rectangles (from Svelte layout)
- [ ] Implement hit-testing in Bevy
- [ ] Forward mouse/keyboard to webview when over UI
- [ ] Handle focus transitions

### Phase 6: IPC Bridge
- [ ] Define command/event message types
- [ ] Implement wry IPC handlers
- [ ] Wire up Svelte stores to send commands
- [ ] Implement dirty flag for on-demand capture

### Phase 7: Svelte UI
- [ ] Build toolbar, side panels with Tailwind
- [ ] Add node graph component (xyflow)
- [ ] Add material editor controls
- [ ] Add diffusion panel

### Phase 8: Diffusion Integration
- [ ] Local: Set up candle with GPU tensor → texture
- [ ] Remote: WebSocket client for streaming bytes
- [ ] Test progressive texture updates during inference

### Phase 9: Polish
- [ ] Performance profiling
- [ ] Error handling
- [ ] Package for distribution

---

## Tradeoffs

| Aspect | Pro | Con |
|--------|-----|-----|
| Native Bevy | Full GPU performance | More complex architecture |
| Offscreen webview | Full Svelte ecosystem | Framebuffer capture overhead |
| On-demand capture | Minimal perf impact | Slight UI update latency |
| Custom render node | Precise control | Requires Bevy render graph knowledge |

### Platform Considerations

| Platform | Webview | Framebuffer Capture | Notes |
|----------|---------|---------------------|-------|
| Linux | WebKitGTK | `webkit_web_view_get_snapshot` | May need GTK main loop integration |
| Windows | WebView2 | `CapturePreview` API | Well-supported |
| macOS | WKWebView | `takeSnapshot` | Well-supported |

### Risks
1. **wry offscreen support** - Not all wry features work headless; may need patches
2. **Framebuffer capture API** - Platform-specific; Linux is most complex
3. **Input hit-testing** - Need to keep UI regions in sync with Svelte layout
4. **GPU memory** - Large UI textures (4K displays) need management

---

## Build Commands

```bash
# Build Svelte UI
cd ui && npm run build

# Build and run Bevy app
cargo run --release -p app

# Development (watch mode)
# Terminal 1: cd ui && npm run dev
# Terminal 2: cargo run -p app -- --dev  # loads from localhost:5173
```

---

## Verification

1. **Single window**: `wmctrl -l` shows only one window
2. **UI rendering**: Svelte panels appear with correct transparency
3. **Input routing**: Click on panel → UI responds; click on 3D → camera orbits
4. **On-demand capture**: UI only re-renders when interacting with it
5. **Diffusion streaming**: Image progressively appears on 3D surface
6. **Performance**: 60fps 3D rendering while UI is idle
