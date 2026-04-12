# Input Handling Architecture

This module forwards Bevy input events to frontend backends (WebKit, CEF,
Dioxus) and accounts for the experimental egui path.

## FrontendBackend SystemParam

`FrontendBackend` in `backend.rs` provides a unified interface for input forwarding.
It handles two different frontend architectures:

### Capture-Based Frontends (WebKit, CEF, Overlay)

- Use `FrontendResource` wrapping `Box<dyn CompositeBackend>`
- Events sent directly to webview via IPC/FFI
- Share framebuffer capture rendering model

### GPU-Native Frontend (Dioxus)

- Uses separate `DioxusRendererResource`
- Events forwarded via mpsc channel to BlitzDocument
- Uses Vello for direct GPU rendering (no capture)

### Bevy-Integrated Frontend (egui)

- Uses `bevy_egui` directly inside the Bevy app
- Does not use `FrontendResource` or the Dioxus renderer resource
- Reuses hotkey state and mouse tracking but does not forward input through a
  separate backend bridge

## Resource Access Rules

**All frontend resources are NonSend** because they contain thread-local types:

- GTK/WebKit widgets (!Send)
- mpsc::Receiver (!Send)
- CEF browser handles (!Send)

Always use `NonSendMut<T>` to access frontend resources, never `ResMut<T>`.

## Adding a New Frontend

For capture-based: Implement `CompositeBackend` trait, add to `create_frontend()`.
For GPU-native: Follow the Dioxus pattern with separate resource type.
