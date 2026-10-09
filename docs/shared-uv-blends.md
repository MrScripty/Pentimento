# UV layer blend modes

The shared UV stack supports Normal, Multiply, Screen and Overlay. Layer blend mode is metadata: changing it does not rewrite raw paint. The active layer's selector is acknowledged by the editor and is unavailable during an active stroke, pending Apply, staged preview or external-owner conflict. Paint lock prevents content painting while allowing metadata edits; hidden layers do not contribute to the stack and refuse painting.

## Linear color and alpha contract

Authoring pixels are associated (premultiplied) linear RGB with alpha. Layer opacity scales all four source channels once. Blend functions use straight linear colors only in the overlap of source and backdrop. For source alpha `as`, backdrop alpha `ab`, associated colors `Ps`, `Pb`, and straight colors `Cs`, `Cb`:

```text
Ao = as + ab * (1 - as)
Po = (1 - as) * Pb + (1 - ab) * Ps + as * ab * B(Cb, Cs)
Normal:   B = Cs
Multiply: B = Cb * Cs
Screen:   B = Cb + Cs - Cb * Cs
Overlay:  B = 2 * Cb * Cs                 when Cb <= 0.5
          B = 1 - 2 * (1 - Cb) * (1 - Cs) otherwise
```

The [W3C compositing and blending equations](https://www.w3.org/TR/compositing-1/#generalformula) define these separable functions and source-over operation. This editor explicitly evaluates them in its existing linear working space, rather than claiming the appearance of an sRGB-working-space image editor. Overlay's branch is chosen by the backdrop channel. The original Normal source-over arithmetic is retained exactly. Non-Normal output is bounded to associated alpha only for floating-point roundoff. Transparent source RGB contributes nothing; transparent backdrop RGB is suppressed without division. Latent transparent raw RGB remains unchanged in authoring data, history and files.

The stack is an **isolated UV group**: its initial backdrop is transparent, and each visible layer blends with visible lower UV layers. The completed group is then Normal-composited over the original material/texture using the unchanged display conversion. A lowest or lone Multiply, Screen or Overlay layer behaves like Normal; its mode does not blend directly against the original material. Put colored paint on a lower UV layer to supply a blend backdrop. Original RGBA8 sRGB textures are decoded by the existing display path and output is unpremultiplied and encoded to RGBA8 sRGB only for the derived display image.

DirectUV brush/erase and Canvas projection Apply continue to author the selected raw layer with their existing stroke/source-over semantics. Layer blend mode is applied by `UvLayersDocument::composite_active` to both accepted pixels and active/live replacement pixels. The mesh uploader, preview, reopened project and ownership validation use this same core path; no competing display compositor or layer shader is introduced.

## Persistence and history

Canonical project version remains v3. The metadata enum stores `Normal`, `Multiply`, `Screen` or `Overlay`; absent fields deserialize to Normal, and Normal fields are omitted on writing. All-Normal edits use `linear-premultiplied-normal-v1`. Any non-Normal mode, including on a hidden or zero-opacity layer, requires `linear-premultiplied-separable-v1`. Unknown enum values/policy markers and non-Normal data under the Normal-only marker refuse before installation. An older Normal-only reader refuses the new marker/metadata instead of silently changing appearance.

Mode edits capture metadata and the compositor policy together in bounded UV history. Undo/Redo restores both without copying or modifying raw layer pixels. Duplicate preserves mode; create defaults to Normal. No-op and rejected mode changes preserve Redo; an accepted new mode edit clears it. Removing the last non-Normal mode returns to the Normal policy; Undo restores the previous policy. Selection and per-layer paint locks retain their existing behavior. Existing history/pending payload and file validation limits still apply; metadata accounting includes the added enum/policy string.

## Verification and native limits

Core tests cover fixed translucent reference vectors, a separately arranged f64 two-stage reference over alpha/color grids, transparent latent RGB, subnormal edges, opacity, order, visibility/lock, active pixel replacement, history and old-v3 defaults. App tests drive real dispatch, native window input forwarding, CPU image updates, DirectUV, Canvas Apply, live preview/Apply/Cancel and atomic owned Save/Open. The existing rendered Svelte harness additionally checks the selector, mode Undo/Redo, preview disabling and saved mode restoration. All evidence and build output stay outside Git; displayed diagnostic screenshots use JPEG85.

These tests qualify CPU authoring/composition and editor routing. Native desktop CEF embedding, GPU extraction/material shading and physical stylus hardware remain unqualified. No GPU parity is asserted. Modes beyond this set, masks, PTex rendering/persistence and automatic admission of newly added receivers remain unsupported.
