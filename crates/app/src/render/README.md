# Frontend Render Architecture

This module manages UI rendering across different frontend technologies.

## Two Rendering Models

### Model 1: Framebuffer Capture (WebKit, CEF, Overlay)

```
Webview renders HTML/CSS/JS
     |
     v
Framebuffer captured (BGRA pixels)
     |
     v
Copied to Bevy Image texture
     |
     v
Displayed via ImageNode overlay
```

All capture-based backends implement `CompositeBackend` trait and are wrapped
in `FrontendResource`. They share:

- Texture creation/resize logic
- Framebuffer-to-texture copy
- Polling and lifecycle management

### Model 2: GPU Native (Dioxus)

```
Dioxus VirtualDom diffing
     |
     v
Blitz layout engine
     |
     v
Vello GPU rasterization
     |
     v
Direct render to Bevy texture
```

Dioxus uses a separate `DioxusRendererResource` because:

- No framebuffer capture needed
- Different event model (channel-based)
- Uses Bevy's render graph for GPU integration

## Why Two Models?

The architectures are fundamentally different:

- Capture: CPU-based, works with any webview technology
- GPU Native: Performance-optimized, tighter Bevy integration

Unifying them under one trait would create leaky abstractions where methods
don't map naturally (e.g., `capture_if_dirty()` for GPU rendering).

## Resource Insertion

All frontend resources are **NonSend** (use `insert_non_send_resource()`):

- WebKit/GTK requires main thread
- CEF browser handles are thread-local
- Dioxus uses mpsc channels

Access via `NonSendMut<T>`, not `ResMut<T>`.
