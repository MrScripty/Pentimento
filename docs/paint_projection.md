# Projection Painting Architecture

This document describes the projection painting system in Pentimento, which allows users to paint on a 2D canvas and have that paint projected onto 3D geometry in the scene.

## Overview

Projection painting works by treating the 2D canvas as a "projector". When enabled, the system casts rays from the camera position through each painted pixel on the canvas and applies the paint color to any 3D mesh surfaces those rays intersect.

```
Camera Position
      |
      |  (ray through canvas pixel)
      v
  +-------+
  | Canvas|  (2D paint surface)
  +-------+
      |
      v
  /-------\
 /  Mesh   \  (3D geometry receives projected paint)
 \---------/
```

## Two Projection Modes

### Mode A: Paint-then-Project
1. User creates a canvas plane (camera locks to fixed position)
2. User paints on the canvas using standard 2D tools
3. User clicks "Project to Scene" (P button)
4. Canvas contents are projected onto all visible meshes in a single pass

### Mode B: Live Projection
1. User creates a canvas plane (camera locks)
2. User enables "Live Projection" toggle (L button)
3. As user paints, strokes project to meshes in real-time
4. Both the canvas and meshes show the paint result

## Architecture

### Key Components

```
┌─────────────────────────────────────────────────────────────────┐
│                         UI Layer                                 │
│  ┌──────────────────┐  ┌──────────────────┐                     │
│  │ paint_toolbar.rs │  │    bridge.rs     │                     │
│  │ - L button       │──│ - set_live_proj  │                     │
│  │ - P button       │  │ - project_scene  │                     │
│  └──────────────────┘  └────────┬─────────┘                     │
└─────────────────────────────────┼───────────────────────────────┘
                                  │ IPC Messages
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Scene Layer                               │
│  ┌────────────────────┐  ┌─────────────────────────────────┐    │
│  │ projection_mode.rs │  │   projection_painting.rs        │    │
│  │ - ProjectionMode   │  │   - ProjectionTargets           │    │
│  │ - ProjectionTarget │  │   - MeshRaycastCache            │    │
│  │ - ProjectionEvent  │  │   - project_canvas_to_scene()   │    │
│  └────────────────────┘  └─────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Painting Layer                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │   raycast.rs    │  │  projection.rs  │  │projection_target│  │
│  │ - Moller-       │  │ - canvas_uv_to  │  │ - UvAtlasTarget │  │
│  │   Trumbore      │  │   _ray()        │  │ - PtexTargetStub│  │
│  │ - MeshHit       │  │ - brush size    │  │                 │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### File Locations

| File | Purpose |
|------|---------|
| `crates/scene/src/projection_mode.rs` | State management (resources, components, events) |
| `crates/scene/src/projection_painting.rs` | Core projection systems and GPU texture management |
| `crates/painting/src/raycast.rs` | Ray-triangle intersection (Moller-Trumbore algorithm) |
| `crates/painting/src/projection.rs` | Math utilities for canvas-to-world projection |
| `crates/painting/src/projection_target.rs` | Storage abstraction trait and implementations |
| `crates/ipc/src/lib.rs` | IPC message types for UI communication |
| `crates/dioxus-ui/src/components/paint_toolbar.rs` | UI buttons for projection controls |

## Data Flow

### Project-to-Scene Flow

```
1. User clicks "P" button
   │
   ▼
2. UI sends PaintCommand::ProjectToScene via IPC
   │
   ▼
3. handle_projection_events() receives ProjectionEvent::ProjectToScene
   │
   ▼
4. project_canvas_to_scene() is called:
   │
   ├─► Get camera position (locked during paint mode)
   ├─► Get canvas plane transform (position, orientation)
   ├─► For each pixel (px, py) in canvas:
   │     │
   │     ├─► Get pixel color from PaintingPipeline
   │     ├─► Skip if transparent (alpha < 0.01)
   │     ├─► Convert pixel to canvas UV
   │     ├─► Create ray: camera → canvas point → into scene
   │     │
   │     ├─► For each mesh with ProjectionTarget component:
   │     │     ├─► Transform ray to mesh local space
   │     │     ├─► raycast_mesh() → find triangle hit
   │     │     └─► Track nearest hit (depth sorting)
   │     │
   │     └─► Apply paint to nearest hit's texture:
   │           ├─► Get UV coordinate from MeshHit
   │           └─► UvAtlasTarget::apply_projected_pixel()
   │
   ▼
5. Dirty regions are uploaded to GPU textures
```

## Key Data Structures

### MeshHit (from raycast.rs)

Contains all information about where a ray hit a mesh:

```rust
pub struct MeshHit {
    pub world_pos: Vec3,      // World-space hit position
    pub face_id: u32,         // Triangle index (for PTex)
    pub barycentric: Vec3,    // Barycentric coords (u, v, w)
    pub normal: Vec3,         // Interpolated surface normal
    pub tangent: Vec3,        // Tangent vector
    pub bitangent: Vec3,      // Bitangent vector
    pub uv: Option<Vec2>,     // Interpolated UV (if mesh has UVs)
}
```

### ProjectionTarget (Component)

Marks a mesh as a target for projection painting:

```rust
#[derive(Component)]
pub struct ProjectionTarget {
    pub storage_mode: MeshStorageMode,  // UvAtlas or Ptex
    pub texture_handle: Option<Handle<Image>>,
    pub dirty: bool,
}
```

### ProjectionTargetStorage (Trait)

Abstraction for different paint storage backends:

```rust
pub trait ProjectionTargetStorage {
    fn storage_mode(&self) -> MeshStorageMode;
    fn hit_to_tex_coord(&self, hit: &MeshHit) -> Option<Vec2>;
    fn apply_projected_pixel(&mut self, tex_coord: Vec2, color: [f32; 4], ...);
    fn apply_projected_dab(&mut self, tex_coord: Vec2, radius: f32, ...);
    fn take_dirty_regions(&mut self) -> Vec<DirtyRegion>;
}
```

## Ray-Mesh Intersection

The system uses the **Moller-Trumbore algorithm** for ray-triangle intersection:

1. For each triangle in the mesh, compute intersection with the ray
2. Return barycentric coordinates (u, v) and distance t
3. Use barycentric coords to interpolate vertex attributes (UV, normal)

```rust
// Simplified Moller-Trumbore
let edge1 = v1 - v0;
let edge2 = v2 - v0;
let pvec = ray_dir.cross(edge2);
let det = edge1.dot(pvec);
// ... compute u, v, t
```

Performance optimization: `MeshRaycastCache` stores extracted mesh data to avoid repeated asset lookups.

## Coordinate Systems

### Canvas UV to World Ray

```
Canvas Pixel (px, py)
        │
        ▼ pixel_to_canvas_uv()
Canvas UV (0-1, 0-1)
        │
        ▼ canvas_uv_to_world()
World Position on Canvas Plane
        │
        ▼ normalize(world_pos - camera_pos)
Ray Direction
```

### Mesh Local Space Transformation

Rays must be transformed to mesh local space before intersection:

```rust
let inv_transform = mesh_transform.affine().inverse();
let local_origin = inv_transform.transform_point3(ray_origin);
let local_dir = inv_transform.transform_vector3(ray_dir).normalize();
```

Hit results are transformed back to world space for depth comparison.

## PTex Integration

The architecture is designed to support PTex (per-face texturing) for meshes without UVs:

- `MeshHit` includes `face_id` and `barycentric` coordinates
- `MeshStorageMode::Ptex` variant exists
- `PtexTargetStub` implements `ProjectionTargetStorage` (placeholder)
- `barycentric_to_ptex_coords()` helper function exists

When the PTex system is implemented (by another agent), it should:
1. Implement `PtexTarget` struct with per-face tile storage
2. Implement `ProjectionTargetStorage` trait
3. Handle the face_id → tile mapping

## GPU Texture Upload

Projected paint uses the same tile-based dirty tracking as canvas painting:

1. `UvAtlasTarget` wraps a `TiledSurface`
2. Paint operations mark affected tiles as dirty
3. `take_dirty_regions()` extracts changed tile data
4. Data is converted to RGBA8 and uploaded via `wgpu::Queue::write_texture()`

## Usage

### Adding Projection Support to a Mesh

```rust
commands.entity(mesh_entity).insert(ProjectionTarget {
    storage_mode: MeshStorageMode::UvAtlas { resolution: (512, 512) },
    texture_handle: None,
    dirty: false,
});
```

### From the UI

1. Enter paint mode (creates canvas, locks camera)
2. Paint on canvas
3. Click **L** to enable live projection, OR
4. Click **P** to project canvas to scene (one-shot)

## Limitations and Future Work

- **Performance**: Currently iterates all canvas pixels; could be optimized with dirty tile tracking for live mode
- **BVH**: No bounding volume hierarchy for large meshes; brute-force triangle iteration
- **Brush Size**: Single-pixel projection; could project brush dabs for smoother results
- **Occlusion**: Handles depth sorting but no transparency support
- **PTex**: Stub implementation; needs full PTex storage system
