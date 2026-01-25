# Pentimento Painting Plan (libmypaint + CanvasPlane + Ptex)

This plan captures the current agreed scope and design notes for implementing
painting in Pentimento. It includes a detailed stroke/dab message schema
(Drawpile-inspired, P2P-ready) and the CanvasPlane painting pipeline.

## Scope and Constraints

- Brush engine: **libmypaint** (CPU rasterization).
- GPU storage: **Rgba16Float** (16-bit linear for grading safety).
- CPU surface: **16-bit** (to preserve quality during grading).
- GPU blending: Blending occurs on GPU, not CPU.
- UI targets: **Dioxus + Svelte**.
- Sync: **No Iroh implementation here**. Provide clean data structures and
  logging hooks so Iroh can be integrated later without refactoring.
- Mesh editing: **Deferred**. Assume static scene until painting + projection
  are complete.
- Brush presets: **Fixed set** for now. No preset synchronization needed.

## Goals

- Paint onto a camera-locked CanvasPlane in 3D.
- Record strokes as **dab lists** (small packets) suitable for later P2P replay.
- Allow both **CanvasPlane** and **MeshPtex** paint spaces.
- Projection from CanvasPlane -> MeshPtex as a deterministic action.
- Replay raw input through libmypaint (visually identical is sufficient).

## Non-Goals (for now)

- Iroh networking and real-time transport.
- Mesh editing and face-ID remapping.
- Layers/undo/redo.
- GPU brush stamping.
- Byte-perfect cross-platform determinism.

---

## Constants

```rust
/// Maximum canvas size (DCI 1K). Not a magic number - may change.
pub const MAX_CANVAS_SIZE: u32 = 1048;

/// Coordinate scale factor (1/4 pixel precision).
pub const COORD_SCALE: f32 = 4.0;

/// Size scale factor (diameter * 256).
pub const SIZE_SCALE: f32 = 256.0;

/// Maximum delta for i8 encoding.
pub const MAX_XY_DELTA: i8 = 127;
```

---

## Stroke/Dab Message Schema (Drawpile-inspired)

This schema is designed to be compact, replayable, and extendable. It is not
byte-compatible with Drawpile but uses a similar dab delta compression model.

### Coordinate Conventions

- Positions are stored in **fixed-point 1/4 pixel units** (x4).
- Dabs are stored as **int8 deltas** from the previous dab.
- When delta exceeds ±127, **flush the packet** and start a new one with fresh
  baseline coordinates (Drawpile approach).
- Sizes are stored as **diameter * 256** (fixed-point, u32).
- Pressure and speed are **quantized per dab** (u8 or u16; see below).

### Header (per stroke)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrokeHeader {
    /// Schema version
    pub version: u8,
    /// 0 = CanvasPlane, 1 = MeshPtex
    pub space_kind: u8,
    /// plane_id OR mesh_id
    pub space_id: u32,
    /// Unique stroke identifier (for future undo/selective replay)
    pub stroke_id: u64,
    /// Timestamp in milliseconds (for Iroh ordering)
    pub timestamp_ms: u64,
    /// Brush preset id (libmypaint)
    pub tool_id: u32,
    /// Blend mode: normal, erase, etc.
    pub blend_mode: u8,
    /// Color in wgpu-native Rgba16Float compatible format
    pub color: [f32; 4],
    /// Reserved for future (tilt, jitter, etc.)
    pub flags: u8,

    /// Base position for delta compression (fixed-point x4)
    pub base_x: i32,
    /// Base position for delta compression (fixed-point x4)
    pub base_y: i32,

    /// Mesh face id (MeshPtex only, ignored for CanvasPlane)
    pub face_id: u32,
    /// Tile index within face (MeshPtex only)
    pub ptex_tile: u16,

    /// Pressure quantization: 0=none, 1=u8, 2=u16
    pub pressure_quant: u8,
    /// Speed quantization: 0=none, 1=u8, 2=u16
    pub speed_quant: u8,
}
```

### Dab (per dab)

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Dab {
    /// Delta x from previous dab (x4 units, ±127 max)
    pub dx: i8,
    /// Delta y from previous dab (x4 units, ±127 max)
    pub dy: i8,
    /// Brush diameter * 256
    pub size: u32,
    /// Hardness 0..255
    pub hardness: u8,
    /// Opacity 0..255
    pub opacity: u8,
    /// Angle 0..255 (maps to 0..2pi)
    pub angle: u8,
    /// Aspect ratio 0..255
    pub aspect_ratio: u8,
    /// Pressure (u8 or u16 based on header)
    pub pressure: u16,
    /// Speed (u8 or u16 based on header)
    pub speed: u16,
}
```

### Delta Overflow Handling

When a dab's delta exceeds ±127 (the i8 limit), flush the current packet and
start a new one:

```rust
fn can_delta(last: i32, current: i32) -> bool {
    let delta = current - last;
    delta >= -128 && delta <= 127
}

// If !can_delta(), flush current packet and start new one with:
// - base_x/base_y = current position
// - dx/dy = 0 for first dab
```

This matches Drawpile's approach (see brush_engine.c:944-957).

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

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrokePacket {
    pub header: StrokeHeader,
    pub dabs: Vec<Dab>,
}
```

Store these packets locally in a stroke log; later Iroh can stream them.

For future Iroh integration, strokes would be stored as key-value entries:
- Key: `strokes/{space_id}/{stroke_id}`
- Value: Serialized `StrokePacket`
- Author: Peer's public key
- Timestamp: `timestamp_ms` from header

---

## Input Routing

- Painting occurs when the **paint tool is active** (user selects from toolbar).
- Canvas planes are anchored in 3D space.
- Press **Tab** with plane selected to lock camera through the plane for painting.
- Press **Tab** again to exit locked view.
- Ray-plane intersection computes plane-local coordinates for dab generation.

---

## CanvasPlane Painting Pipeline

### 1) Plane Setup

- Create a **CanvasPlane entity** in the 3D scene.
- Align it to the camera for isometric painting:
  - plane normal = camera forward (or a fixed iso direction)
  - plane size = configurable (resolution in pixels, max 1048x1048)
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
- **Validate dabs**: if libmypaint returns invalid parameters, abort the stroke.
  Invalid data must never sync to P2P or save to project.

### 4) CPU Surface + Dirty Tiles

- Maintain a **tiled CPU surface** for the CanvasPlane:
  - tile size (e.g., 128x128) for efficient updates.
  - format: **16-bit** (Rgba16Float or equivalent) for color grading quality.
- Track **dirty tiles** touched by dabs.

### 5) GPU Upload (Bevy Image)

- The CanvasPlane is backed by a Bevy `Image`:
  - `TextureFormat::Rgba16Float`
  - `TEXTURE_BINDING | COPY_DST | STORAGE_BINDING`
- For dirty tiles:
  - Upload via Bevy image update or wgpu queue write.
  - No format conversion needed (CPU and GPU both 16-bit).

### 6) Rendering

- CanvasPlane material samples the GPU texture.
- The texture is always 16-bit linear.
- Blending occurs on GPU shader.
- This path is zero-copy for display (GPU-local texture).

### 7) Stroke Log Output

On stroke end:
- Validate the stroke packet.
- Store `StrokePacket` locally if valid.
- Emit a hook/event so future Iroh integration can transmit it.

On validation failure:
- Abort the stroke.
- Do not sync to P2P or save to project.
- Log the error for debugging.

---

## Error Handling

- **Invalid dab parameters**: Abort stroke, do not sync, log error.
- **Delta overflow**: Flush packet and start new one (not an error).
- **Tile upload failure**: Retry or abort stroke, do not leave partial state.

---

## MeshPtex (Deferred but Planned)

- Same dab schema, but `space_kind = MeshPtex`.
- Use `face_id + (u,v)` fixed-point coords instead of plane coords.
- Projection from CanvasPlane to MeshPtex is a **deterministic command**.
- Mesh editing is out of scope until after painting + projection are complete.

---

## Implementation Progress

### Completed

#### Phase 1: Foundation ✅
- [x] Created `crates/painting/` crate
- [x] Defined `StrokeHeader`, `Dab`, `StrokePacket` structs in `types.rs`
- [x] Added constants in `constants.rs`
- [x] Implemented validation functions in `validation.rs`

#### Phase 2: Parallel Workstreams ✅
- [x] **2A**: Stroke log (`log.rs`) with `StrokeLog`, `StrokeRecorder`, Iroh-ready hooks
- [x] **2B**: CanvasPlane entity (`canvas_plane.rs`) + Tab camera lock + paint mode
- [x] **2C**: CPU tiled 16-bit surface (`surface.rs`, `tiles.rs`) + dirty tracking

#### Phase 3: Integration ✅
- [x] Simple brush engine (`brush.rs`) with spacing-based dab interpolation
- [x] Painting pipeline (`pipeline.rs`) connecting brush → surface → log
- [x] GPU upload system (`painting_system.rs`) with dirty tile tracking
- [x] CanvasPlane material with Rgba8UnormSrgb texture (changed from Rgba32Float)

#### Phase 5: Bug Fixes ✅
Key issues fixed to get painting working:

1. **TextureUsages::TEXTURE_BINDING missing** - GPU textures couldn't be sampled
   by shaders without this flag. Added to both initial setup and dirty uploads.

2. **UV coordinate calculation** - Fixed ray_plane_intersection to use Rectangle
   mesh normals (-Z forward) instead of Plane3d (Y up), and properly scale UVs
   using world_width/world_height dimensions.

3. **Canvas plane orientation** - Rectangle mesh faces +Z, but looking_at makes
   -Z point at target. Added 180° Y rotation so canvas faces the camera.

4. **Linear to sRGB conversion** - CPU surface stores linear RGB but GPU expects
   sRGB for Rgba8UnormSrgb format. Added gamma correction in `surface_to_rgba8()`.

5. **Camera lock during painting** - Added checks in orbit/pan/zoom systems to
   prevent camera movement when `ActiveCanvasPlane.camera_locked` is true.

#### Phase 4: UI ✅
- [x] Shift+A menu with "Paint" option (Dioxus + Svelte)
- [x] `CreateInFrontOfCamera` event spawns plane facing camera
- [x] Auto camera lock and paint mode on canvas creation
- [x] Paint toolbar component (Dioxus + Svelte)
- [x] `EditMode` state and `EditModeChanged` IPC message
- [x] Tab key exits paint mode and unlocks camera

### Known Issues

#### Performance (Complete)
The GPU upload system has been fully optimized:
- ✅ Image handle is reused (no handle/material churn from creating new images)
- ✅ Dirty tiles are tracked per-tile (128x128 regions)
- ✅ True partial tile upload via `wgpu::Queue::write_texture()`
- ✅ Bypasses Bevy's asset system entirely for GPU-level partial updates

**Performance gain:** ~68x reduction in upload size for typical single-tile updates
(65 KB per tile vs 4.4 MB for full 1048x1048 canvas)

#### Brush Feel
The simple `BrushEngine` uses basic spacing interpolation. For production:
- Integrate libmypaint for proper brush dynamics
- Add pressure sensitivity support (tablet input)
- Implement smoothing/stabilization for mouse input

### Remaining Work

#### Performance Optimization (Complete)
- [x] Reuse Image handle instead of creating new one each frame
- [x] Dirty tile tracking per 128x128 tile regions
- [x] True partial tile upload via `wgpu::Queue::write_texture()`

#### libmypaint Integration (Deferred)
- [ ] FFI bindings for libmypaint
- [ ] Replace simple `BrushEngine` with libmypaint
- [ ] Brush preset loading

#### Polish
- [ ] Color picker in paint toolbar
- [ ] Brush size adjustment
- [ ] Eraser tool
- [ ] Undo/redo (deferred per plan)

---

## Files Created

### crates/painting/
- `Cargo.toml` - Package manifest
- `src/lib.rs` - Module exports
- `src/constants.rs` - MAX_CANVAS_SIZE, COORD_SCALE, etc.
- `src/types.rs` - StrokeHeader, Dab, StrokePacket, SpaceKind, BlendMode
- `src/validation.rs` - validate_dab, can_delta, coordinate conversions
- `src/log.rs` - StrokeLog, StrokeRecorder, StrokeLogEvent
- `src/surface.rs` - CpuSurface with [f32; 4] pixels
- `src/tiles.rs` - TiledSurface with dirty tracking, apply_dab
- `src/brush.rs` - BrushPreset, BrushEngine, DabOutput
- `src/pipeline.rs` - PaintingPipeline

### crates/scene/
- `src/canvas_plane.rs` - CanvasPlane, ActiveCanvasPlane, CanvasPlaneEvent
- `src/paint_mode.rs` - PaintMode, PaintEvent, ray-plane intersection
- `src/painting_system.rs` - PaintingResource, CanvasTexture, GPU upload
- `src/edit_mode.rs` - EditModeState, EditModeEvent, EditModePlugin

### crates/ipc/
- Added `EditMode` enum (None, Paint)
- Added `AddPaintCanvasRequest` struct
- Added `BevyToUi::EditModeChanged` message
- Added `UiToBevy::AddPaintCanvas` message

### crates/dioxus-ui/
- `src/components/paint_toolbar.rs` - PaintToolbar component
- Modified `add_object_menu.rs` - Added "Paint" option
- Modified `bridge.rs` - Added `add_paint_canvas` method
- Modified `app.rs` - Added edit_mode state and PaintToolbar

### ui/ (Svelte)
- `src/lib/components/PaintToolbar.svelte` - Paint toolbar
- Modified `AddObjectMenu.svelte` - Added "Paint" option
- Modified `bridge.ts` - Added `addPaintCanvas` method
- Modified `App.svelte` - Added editMode state and PaintToolbar
- Modified `types.ts` - Added EditMode and IPC types

---

## User Flow

```
Shift+A → Menu shows "Paint" option
    ↓
Click "Paint" → CanvasPlane created in front of camera
    ↓
Camera locks, paint mode enabled
    ↓
Paint toolbar appears ("Press Tab to exit")
    ↓
Left-click and drag → Paints on canvas
    ↓
Tab → Exit paint mode, unlock camera, hide toolbar
```
