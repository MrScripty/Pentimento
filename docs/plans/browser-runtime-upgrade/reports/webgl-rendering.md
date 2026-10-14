# WebGL rendering repair

Status: full hosted Electron qualification passed on source head
`e368821a274bd6a544e132500887f81786126c43`, including the genuine scene, two
stable selection-outline frames and graceful process-family shutdown. Earlier
rendering and test-segmentation failures below are historical. The strict
scene-pixel criteria and Electron security settings are unchanged. The subsequent
native Canvas ordering repair creates a new head that still needs its own rerun.

## Observed failure

[Electron job 115320474020](https://github.com/MrScripty/Pentimento/actions/runs/38404508884/job/115320474020)
ran on PR #28's published head `719ac6b`. Its
[retained artifact 11817047866](https://github.com/MrScripty/Pentimento/actions/runs/38404508884/artifacts/11817047866)
contains `production.json`, `production.png`, build logs and sandbox results.
The environment sandbox probe and genuine production WASM build succeeded.
The screenshot shows the real Svelte toolbar/properties panel over a blank
1919×1052 canvas, with zero FPS. Production JSON identifies Electron 44.7.0,
Chromium 152.0.7977.130, WebGL2 through ANGLE/SwiftShader, and Bevy's `Gl` backend.
The document/preload loaded, Node globals were absent, and no remote request was
needed. The scene's real initialization messages precede a fatal render error.

Three concrete validation failures explain the blank scene:

1. The outline ID camera used `Rgba8Unorm`. Bevy 0.20's view preparation adds an
   alternate sRGB view to this linear format. The GL adapter lacks the required
   `VIEW_FORMATS` downlevel capability, invalidating its ping-pong textures.
2. The outline post-process hard-coded `Rgba16Float` while its actual destination
   was `Rgba8UnormSrgb`, making the pipeline incompatible with the render pass.
3. Depth view eagerly compiled a multisampled depth `textureLoad` shader even
   when the feature was off. The pinned Naga 30 GLSL backend explicitly rejects
   depth-texture loads, including single-sample depth loads. Bevy then logged
   that it was quitting after a validation render error.

This was neither evidence of a startup delay nor evidence that the nearby
SwiftShader deprecation warning caused failure. Adding an unsafe flag cannot
repair these texture/shader contracts, and no such flag was added.

## Repair and lineage

The format specialization and four descriptor/ECS regression tests are adapted
from repository commit `534d31d` (`fix(scene): specialize post-process pipelines
by view target format`), retaining the current Bevy 0.20 render schedules and
GPU-resource recovery lifecycle. Each render view owns its prepared pipeline
key and depth input; preparation selects `ViewTarget::main_texture_format()`.
A missing, stale-format, or compiling pipeline skips before any ping-pong swap.
The fullscreen output stays single-sample while native depth input retains its
existing multisampled contract.

The truthful depth capability contract is adapted from reviewed repository
commit `2022d754b393fd1a7ce08205bee955d45bf272d8`. The actual adapter backend owns
availability. The current GL implementation and unknown renderers are unavailable;
Vulkan, Metal, DX12 and BrowserWebGPU retain the existing native-capable path.
Only a supported and enabled depth view queues a depth shader. The browser
previously displayed an optimistic depth toggle while its WASM command handler
only logged and ignored that request. The restored contract reports that real
limitation instead of claiming the browser feature worked or adding a stub.
Consumer recovery, rejection, and native behavior are documented separately in
[the depth capability contract](../../../depth-view-capability.md).

The ID pass now uses a directly renderable `Rgba8UnormSrgb` target with paired
shader conversions:

1. Keep entity identity as three normalized byte channels.
2. Convert sRGB to linear before the attachment write, compensating for the
   attachment's automatic linear-to-sRGB conversion.
3. Preserve those exact bytes in the 8-bit attachment.
4. Sampling decodes sRGB; convert back to the normalized ID representation
   before interpreting ID channels. Alpha coverage is unchanged.

Changing only the texture format would merge high IDs: the review's negative
example found byte values 75 and 76 both quantizing to 148. The compensated
path's CPU regression covers all 256 values in every channel with high-valued
neighboring channels, including storage quantization and decode. The ID data
camera also disables tone mapping, dithering and MSAA to avoid altering or
mixing identities. These tests prove the encoding math; the hosted selection
qualification below separately exercises the actual ID material and outline
path. It does not perform GPU readback of every possible 24-bit identity.

The production Electron test now exercises that ID material by selecting the
actual red scene object with Electron mouse input. Its click is derived from
the largest connected red population in the native capture, mapped from bitmap
coordinates to window DIPs, and checked against the visible canvas. With the
pointer then parked away, two consecutive frames must retain the canonical
scene and gain orange pixels concentrated at that object's original boundary.
The production shader draws inside the silhouette, so the boundary band allows
both sides of the observed material edge. An orange fill, unrelated changed
scene, wrong-object/color outline, or unchanged/hover-only frame cannot pass.
Before/after PNGs and coordinate/pixel metrics are retained. This is a real GPU
ID-pass/outline check; it complements, rather than replaces, the all-channel
ID math regression. The first hosted interaction and its oracle correction are
recorded below.

Production qualification also requires the real WASM capability response to
reach the depth control. It records the disabled/unpressed state and matching
visible accessible OpenGL/WebGL reason in `production.json`; a merely disabled
control still waiting for bootstrap is rejected.

Finally, Electron console observation now recognizes Bevy's formatted ERROR
records even when the browser reports them at `console.info` level. A regression
test uses the observed render-error shape, and a negative case keeps ordinary
software-renderer warnings distinct from fatal evidence. The pixel oracle's
detail/color thresholds and genuine-WASM requirements have not been relaxed.

## Hosted repair evidence and oracle correction

[Run 38410829861, job 115342390453](https://github.com/MrScripty/Pentimento/actions/runs/38410829861/job/115342390453)
used source head `713653f0804255636af5b89e6e6eb1cc12f6e311`. The retained
`environment.log` records PR merge commit `ed01a0a1b6c0b8679e38c3819c32aba39010fa4c`;
GitHub's commit metadata verifies that its tree `a824de470481cde6cde3611bcf8e687c7014fa96`
equals the source head's tree. The
[artifact 11822966280](https://github.com/MrScripty/Pentimento/actions/runs/38410829861/artifacts/11822966280)
contains the real pre/post-selection PNGs and complete JSON/log evidence.

The build and ordinary sandbox probe passed. Both strict scene captures contain
the red cube, green torus, blue sphere and ground. The depth control received the
authoritative disabled OpenGL/WebGL reason, and no Bevy ERROR was observed.
A real click at DIP `(960,457)` selected entity `297`; the retained after image
shows a thin orange outline around that cube with no unrelated scene changes.

The test failed because it reused the bright-red scene-population predicate as
a geometric silhouette mask. Shadowed metallic-face pixels such as RGB
`(71,32,49)` failed its red/blue ratio, fragmenting the baseline component into
4,799 artificial boundary pixels. Of 1,842 new exact-orange pixels, 512 legitimate
side/bottom outline pixels lay outside that fragmented mask's narrow band.

The correction is only to this canonical red object's geometry predicate:
retain strong red/green dominance and admit the observed red-dominant shadow
against blue (`r > b * 1.1 + 8`). The original `inspectScene` palette predicate,
four-pixel boundary band, 80% localization requirement, fill rejection, and
outside-change limit are unchanged. The baseline then has one coherent
39,750-pixel silhouette and 682 boundary pixels. Local and independent-review
replay of the exact retained images places all 1,842 new orange pixels in that
band, with zero orange interior pixels and zero meaningful outside changes
among 1,225,275 outside pixels. A regression fixture uses that actual shadowed
color; neutral, green, blue, wrong-object/color, fill and global-tint negatives
remain rejected.

This replay established the bounded oracle diagnosis, not a successful hosted
job. In that historical run the failed assertion prevented the production
shutdown handshake; the process family exited without forced cleanup. The
corrected full run subsequently passed, as recorded below.

## Successful corrected run

[Run 38414235793](https://github.com/MrScripty/Pentimento/actions/runs/38414235793)
completed successfully on source `e368821a274bd6a544e132500887f81786126c43`.
Its recorded PR merge checkout `e25f035a7d65259311421cb85f9f83954365d293`
has the same tree `64d9f9ac1a31ff9cb28b201380eb7d91a4cd9b50`.
The inspected
[artifact 11824680772](https://github.com/MrScripty/Pentimento/actions/runs/38414235793/artifacts/11824680772)
records two accepted consecutive selection frames after a real click at DIP
`(960,464)`: all 1,842 new orange pixels lie on the cube boundary, with no
orange fill or meaningful outside changes. The genuine scene and authoritative
depth-unavailable UI checks passed, with no renderer errors or network requests.
Production recorded `willQuit: true`; supervision recorded exit 0, a completed
handshake, no surviving processes and no forced cleanup.

The [Electron qualification report](electron-qualification.md) provides exact
environment, sandbox observations, artifact digest and coverage. This qualifies
the software-rendered Ubuntu 22.04 Electron path at the identified source tree,
not hardware performance, browser depth visualization or the separate native
paint/sculpt workflow. The ongoing native Canvas ordering repair needs a fresh
qualification at its new source head.

## Verification

- Twenty-one shell/render JavaScript tests passed after these changes, including
  formatted Bevy errors, rejected pending depth bootstrap, the unchanged
  scene-pixel fixtures, shaded-face geometry, and positive/negative selection
  boundary fixtures. JavaScript syntax and whitespace checks also pass.
- Four format descriptor/view-association tests, four depth capability tests,
  and the all-channel ID roundtrip test are present for native Rust execution.
- The canonical WASM build and complete Electron runtime qualification passed
  at the source tree identified above. Native test and workflow evidence is
  tracked separately; later source changes do not inherit this exact-head pass.
- No hardware-GPU performance, browser depth visualization, or unsupported
  platform claim is introduced.
