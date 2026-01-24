# Dioxus UI Architecture: Shared wgpu with Bevy

This document explains how the Dioxus UI integrates with Bevy through shared wgpu resources and the implications for rendering.

## Overview

The Dioxus UI uses Vello as its GPU renderer, which shares the same wgpu device and queue as Bevy. This enables zero-copy texture sharing but requires careful handling of color spaces and alpha blending.

## Shared wgpu Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      wgpu Device                            │
│                    (shared instance)                        │
├─────────────────────────┬───────────────────────────────────┤
│      Bevy Renderer      │        Vello Renderer             │
│   - 3D scene rendering  │   - 2D UI vector graphics         │
│   - Post-processing     │   - Compute shader pipeline       │
│   - UI compositing      │   - Direct texture output         │
└─────────────────────────┴───────────────────────────────────┘
                          │
                          ▼
              ┌───────────────────────┐
              │   Shared GPU Texture  │
              │   (Rgba8Unorm format) │
              │   STORAGE_BINDING     │
              └───────────────────────┘
```

### Key Components

1. **Bevy's RenderDevice**: Provides the wgpu `Device` that both Bevy and Vello use
2. **SharedVelloRenderer**: Thread-safe wrapper (`Arc<Mutex<Renderer>>`) for Vello in Bevy's render world
3. **GpuImage**: Bevy's texture that Vello renders directly to (zero-copy)

## Texture Format Constraints

### Why `Rgba8Unorm` (Linear)?

Vello's compute shaders require `STORAGE_BINDING` on the output texture. This limits the available texture formats:

| Format | STORAGE_BINDING | Notes |
|--------|-----------------|-------|
| `Rgba8Unorm` | ✅ Supported | Linear color space |
| `Rgba8UnormSrgb` | ❌ Not supported | Would be ideal for display |

The Svelte/WebKit UI uses `Rgba8UnormSrgb` because it uploads via CPU (`COPY_DST`), not compute shaders.

### Implications

1. **Linear color space**: Vello outputs linear RGB values
2. **Shader conversion required**: Must convert linear → sRGB in the compositing shader
3. **Alpha handling**: Vello uses straight alpha internally

## Custom UiMaterial for Compositing

Because of the linear color space and storage texture constraints, we cannot use Bevy's default `ImageNode` for proper transparency. Instead, we use a custom `UiMaterial` with a shader that handles:

### 1. Color Space Conversion

```wgsl
// Convert from linear to sRGB for display
let srgb = pow(color.rgb, vec3(1.0 / 2.2));
```

### 2. Alpha Blending

Bevy's `UiMaterial` pipeline uses `BlendState::ALPHA_BLENDING` by default:

```rust
// Blend formula (non-premultiplied):
// output.rgb = src.rgb * src.a + dst.rgb * (1 - src.a)
// output.a   = src.a + dst.a * (1 - src.a)
```

The shader must output **straight alpha** (not premultiplied):

```wgsl
// Correct: straight alpha
return vec4(srgb, color.a);

// Wrong: premultiplied (would double-multiply alpha)
return vec4(srgb * color.a, color.a);
```

## Comparison with Svelte UI

| Aspect | Svelte (WebKit) | Dioxus (Vello) |
|--------|-----------------|----------------|
| Renderer | WebKit browser engine | Vello GPU compute |
| Texture format | `Rgba8UnormSrgb` | `Rgba8Unorm` |
| Texture usage | `COPY_DST` | `STORAGE_BINDING` |
| Data transfer | CPU → GPU upload | Zero-copy GPU |
| Color space | sRGB (native) | Linear (needs conversion) |
| Alpha format | Straight (Cairo unpremultiplies) | Straight |
| Compositing | Default `ImageNode` | Custom `UiMaterial` |

## Implementation Files

- `crates/app/src/render/ui_dioxus.rs` - Main plugin and systems
- `crates/app/src/render/ui_blend_material.rs` - Custom UiMaterial
- `crates/app/src/render/shaders/ui_blend.wgsl` - Compositing shader
- `crates/dioxus-ui/src/renderer.rs` - Vello scene building

## Thread Safety

Vello's `Renderer` is not `Send + Sync`, but Bevy's render world requires it. Solution:

```rust
pub struct SharedVelloRenderer(Arc<Mutex<Renderer>>);
```

The mutex is only held during the single `render_to_texture` call per frame, so contention is minimal.

## Performance Characteristics

- **Zero-copy**: Vello renders directly to Bevy's `GpuImage` texture
- **Single GPU context**: No device synchronization overhead
- **Compute pipeline**: Vello uses GPU compute shaders (not rasterization)
- **Frame pipelining**: UI texture ready for same-frame compositing
