# Pentimento Painting Plan (libmypaint + CanvasPlane + Ptex)

This plan captures the current agreed scope and design notes for implementing
painting in Pentimento. It includes a detailed stroke/dab message schema
(Drawpile-inspired, P2P-ready) and the CanvasPlane painting pipeline.

## Scope and Constraints

- Brush engine: **libmypaint** (CPU rasterization).
- GPU storage: **Rgba16Float** (16-bit linear for grading safety).
- UI targets: **Dioxus + Svelte**.
- Sync: **No Iroh implementation here**. Provide clean data structures and
  logging hooks so Iroh can be integrated later without refactoring.
- Mesh editing: **Deferred**. Assume static scene until painting + projection
  are complete.

## Goals

- Paint onto a camera-locked CanvasPlane in 3D.
- Record strokes as **dab lists** (small packets) suitable for later P2P replay.
- Allow both **CanvasPlane** and **MeshPtex** paint spaces.
- Projection from CanvasPlane -> MeshPtex as a deterministic action.

## Non-Goals (for now)

- Iroh networking and real-time transport.
- Mesh editing and face-ID remapping.
- Layers/undo/redo.
- GPU brush stamping.

---

## Stroke/Dab Message Schema (Drawpile-inspired)

This schema is designed to be compact, replayable, and extendable. It is not
byte-compatible with Drawpile but uses a similar dab delta compression model.

### Coordinate Conventions

- Positions are stored in **fixed-point 1/4 pixel units** (x4).
- Dabs are stored as **int8 deltas** from the previous dab.
- Sizes are stored as **diameter * 256** (fixed-point, uint24).
- Pressure and speed are **quantized per dab** (u8 or u16; see below).

### Header (per stroke)

```text
StrokeHeader {
  version: u8                    // schema version
  space_kind: u8                 // 0 = CanvasPlane, 1 = MeshPtex
  space_id: u32                  // plane_id OR mesh_id
  tool_id: u32                   // brush preset id (libmypaint)
  blend_mode: u8                 // normal, erase, etc.
  color_rgba: u32                // RGBA8 or ARGB8 (decide encoding early)
  flags: u8                      // reserved for future (tilt, jitter, etc.)

  // Base position for delta compression
  base_x: i32                    // fixed-point x4
  base_y: i32                    // fixed-point y4

  // Optional: if MeshPtex
  face_id: u32                   // mesh face id (static for now)
  ptex_tile: u16                 // optional: tile index within face

  // Quantization settings (so peers can decode)
  pressure_quant: u8             // 0=none, 1=u8, 2=u16
  speed_quant: u8                // 0=none, 1=u8, 2=u16
}
```

### Dab (per dab)

```text
Dab {
  dx: i8                         // delta x (x4 units)
  dy: i8                         // delta y (x4 units)
  size: u24                      // diameter * 256
  hardness: u8                   // 0..255
  opacity: u8                    // 0..255
  angle: u8                      // 0..255 (0..2pi)
  aspect_ratio: u8               // 0..255
  pressure: u8|u16               // quantized per dab
  speed: u8|u16                  // quantized per dab
}
```

### Quantization Notes

- **u8** is compact and likely sufficient (0..255). Good for P2P bandwidth.
- **u16** gives headroom for stylus devices and later smoothing.
- The header advertises quantization so peers can decode correctly.
- Use deterministic rounding for cross-machine stability.

### Space-specific Notes

- **CanvasPlane**: `base_x/base_y` are plane-local fixed-point coords.
- **MeshPtex**: `base_x/base_y` are face-local Ptex coords (fixed-point),
  plus `face_id`. Tile index is optional; can be derived from face + uv.

### Packet Framing (Iroh-ready hook)

```text
StrokePacket {
  header: StrokeHeader
  dabs: [Dab]
}
```

Store these packets locally in a stroke log; later Iroh can stream them.

---

## CanvasPlane Painting Pipeline

### 1) Plane Setup

- Create a **CanvasPlane entity** in the 3D scene.
- Align it to the camera for isometric painting:
  - plane normal = camera forward (or a fixed iso direction)
  - plane size = configurable (resolution in pixels)
- Provide a **plane_id** for stroke headers.

### 2) Input Capture

- On pointer down: begin a stroke.
- Track pointer moves in world space.
- Compute **ray-plane intersection** to obtain plane-local coords.
- Compute **pressure** (mouse = 1.0) and **speed** (delta distance / delta time).
- Quantize `(x, y, pressure, speed)` for dab generation.

### 3) Dab Generation (libmypaint)

- Feed input samples into libmypaint with:
  - position
  - pressure
  - speed
  - brush preset
- libmypaint produces **dabs** on a CPU surface.
- Record each dab into the stroke log as `Dab { dx, dy, size, ... }`.

### 4) CPU Surface + Dirty Tiles

- Maintain a **tiled CPU surface** for the CanvasPlane:
  - tile size (e.g., 128x128) for efficient updates.
  - format: RGBA16F target (or RGBA8 for CPU, then convert).
- Track **dirty tiles** touched by dabs.

### 5) GPU Upload (Bevy Image)

- The CanvasPlane is backed by a Bevy `Image`:
  - `TextureFormat::Rgba16Float`
  - `TEXTURE_BINDING | COPY_DST | STORAGE_BINDING`
- For dirty tiles:
  - Convert CPU pixels to Rgba16Float if needed.
  - Upload via Bevy image update or wgpu queue write.

### 6) Rendering

- CanvasPlane material samples the GPU texture.
- The texture is always 16-bit linear.
- This path is zero-copy for display (GPU-local texture).

### 7) Stroke Log Output

On stroke end:
- Store `StrokePacket` locally.
- Emit a hook/event so future Iroh integration can transmit it.

---

## MeshPtex (Deferred but Planned)

- Same dab schema, but `space_kind = MeshPtex`.
- Use `face_id + (u,v)` fixed-point coords instead of plane coords.
- Projection from CanvasPlane to MeshPtex is a **deterministic command**.
- Mesh editing is out of scope until after painting + projection are complete.

---

## Implementation Checklist (Short-term)

1) Define Rust structs for `StrokeHeader`, `Dab`, `StrokePacket`.
2) Add stroke log storage and hooks for future Iroh.
3) Implement CanvasPlane entity + ray-plane mapping.
4) Integrate libmypaint: generate dabs + record stroke packets.
5) Add CPU tiled surface + dirty tile tracking.
6) Upload tiles to a Bevy `Image` (Rgba16Float).
7) Render the CanvasPlane in the Bevy scene.

