# Canvas projection painting

Projection painting transfers the active 2D canvas to UV-mapped scene meshes.
The native scene plugin implements the same path for supported frontends.

## User workflow

1. Enter canvas paint mode, which creates a canvas and locks the camera.
2. Paint on the canvas using the existing brush and layer tools.
3. Use **Project to Scene** to refresh that canvas's projection once, or enable
   **Live Projection** to refresh it when its composited pixels change.
4. In live mode, canvas erase, stroke cancel, undo, layer opacity, and visibility
   changes are reflected on the mesh. Each canvas retains its own projection
   layer. Repeating Project does not repeatedly darken translucent paint.

Projection layers are composited in canvas creation order. Refreshing a canvas
replaces that canvas's previous projection, including when its projection view
or a mesh moves. A canvas with a saved paint view keeps that view when the user
unlocks the camera to inspect the scene. A different canvas adds a separate layer. ClearProjection removes all
canvas projections from a mesh; ClearAllProjections removes them from every mesh.
A clear remains clear until source pixels, geometry, or the projection view
changes, or Project is explicitly used.

## Supported targets

Triangle-list meshes with UV0 and a StandardMaterial are registered automatically
at 512×512. An explicit ProjectionTarget::uv_atlas controls the atlas resolution.
Indexed and non-indexed triangles are supported. Canvas planes are excluded.
Hidden meshes are excluded; visible meshes without usable UVs still occlude
projection onto meshes behind them. Material front/back/no-culling settings are
respected for both receiving surfaces and occluders, using geometric winding
after world transforms, including reflected objects and inverted camera culling.
The lighting-only `double_sided` flag does not disable culling.

UV coordinates keep their existing meaning: v=0 addresses the top image row.
Atlas texels do not wrap beyond 0–1. Existing mirrored/overlapping UV islands still
share texture texels; projection does not rewrite mesh geometry or unwrap UVs.
Nonuniform and reflected world transforms are supported. Surface normals use the
inverse-transpose transform.

PTex projection is unsupported. The compatibility placeholder rejects hits,
and the scene does not manufacture an approximate PTex atlas. Materials using
UV1 or a nonidentity UV transform are also left unchanged with a diagnostic.
Base textures must be CPU-readable RGBA8/BGRA8, in linear or sRGB format. If a
base image is unavailable or unsupported, the original material is preserved,
and pending paint is retained rather than silently destroying the texture.

## Data path

The ProjectionPaintingPlugin runs in PostUpdate after transform and inherited
visibility propagation and after the canvas pipeline's Update systems.

1. Extract/validate triangle geometry, preserving original vertices and UVs.
2. Rasterize each target triangle in atlas space. For each covered texel, find
   its world position, intersect the camera ray with the canvas plane, and find
   the corresponding canvas pixel.
3. Test the sample against the nearest scene triangle using a projection-local
   bounding-volume tree. Occluded texels and geometry in front of the canvas
   receive no projection.
4. Cache this atlas-to-canvas mapping until geometry, transforms, visibility,
   resolution, material culling, or projection view changes. Mesh asset modifications invalidate
   cached geometry. Pixel edits reuse the mapping.
5. Replace the active canvas's projection layer from the current composited
   canvas. Source snapshots prevent unnecessary work or repeated alpha blending.
6. Composite canvas projection layers, dirtying only changed atlas pixels.
7. Composite dirty image rectangles over the original material in linear color
   space, then encode sRGB RGBA8. Bevy owns Image extraction and GPU upload, so
   creating a texture before its GPU asset is ready does not lose updates.
8. Bind a private material copy only when paint exists. Unpainted texels preserve
   the original color/texture; clearing all paint restores the original handle.
   A mesh sharing its original material with another entity does not recolor it.
   Image changes also invalidate the private material binding so it samples the
   updated GPU texture view rather than a cached previous view.

The Image asset path updates changed CPU rectangles; Bevy may upload the full
image. This is not a custom partial-write GPU upload implementation.

## Relevant files

- crates/scene/src/projection_painting.rs: registration, cached UV coverage,
  occlusion, per-canvas layers, material/image output, integration tests
- crates/scene/src/projection_mode.rs: commands and enabled state
- crates/painting/src/projection_target.rs: UV storage and dirty regions
- crates/scene/src/painting_system.rs: source canvas pipeline and undo

## Verification

- cargo test -p painting projection_target
- cargo test -p pentimento-scene projection_painting --lib

Coverage includes UV corner orientation and inclusive edges, invalid UV rejection,
nonuniform normals, material-aware facing, reflected geometry, inverted-camera
culling, rasterization without holes, UV-less occluders, packed dirty
edge rectangles, live Image/material output, original/shared-material preservation,
repeat-project idempotence, command ordering, clear, source-stroke undo, and
material binding invalidation after image updates. These headless Bevy
system tests validate the CPU and asset path; rendered frontend acceptance is a
separate check.
