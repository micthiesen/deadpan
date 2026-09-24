# Framing renderer and plan assessment

Read-only assessment on 2026-09-24. No repository files changed, tests run, live
GPU opened, or new dependency research performed. This is a proposed contract,
not implemented or qualified behavior. The core author owns the final authored
model. The final recommendation below incorporates our agreement on one explicit
canvas-space Framing per node, with source-space targets converted at selection.

## Existing path and precise gaps

- `docs/spec/DEADPAN_SPEC.md:414` requires Camera movement in percentages of the
  uncropped source, multiplicative scale, one Enter commit and Escape restoration.
  `:859` orders source interpretation before editorial framing and group effects,
  and requires shared preview/export math. `:616` defines source-time targets;
  `:660` requires generation before editorial zoom. `:1109` preserves the Original's
  display geometry and makes generated/crop fit policy explicit.
- `crates/deadpan-render/src/geometry.rs:23` already applies SAR along the coded
  horizontal axis, then right-angle rotation, then uniform centered Fit/Fill.
  `source_uv` at `:62` uses output pixel centers and half-open upright bounds,
  followed by the inverse rotation into coded source UV. It has no editorial
  transform, canonical project canvas, clipping stack or target forward map.
- `geometry.rs:81` is an f64 CPU reference. It decodes transfer and converts to
  linear Rec.2020 before bilinear filtering, premultiplies source alpha, composites
  over black and emits opaque sRGB. It intentionally does not model half-float
  GPU quantization.
- `gpu.rs:174` always uploads the full immutable RGBA frame, derives geometry from
  the target size and FitMode, submits the interpretation and display passes, and
  allows one submission in flight. `parameters` at `:291` writes eight explicit
  vec4 values. `picture.wgsl` repeats the rectangle/inverse-rotation mapping, uses
  four textureLoad taps, clamps taps after coverage rejection, and discards alpha
  after premultiplication because the only current composition is opaque black.
- `deadpan-plan/src/plan.rs:438` traverses a single indexed active path, starting
  at exact project time frame+1/2. It retains exact Retime mappings, stable Repeat
  identities, gap ownership and leaf local time. `picture.rs:91` returns only the
  selected provider and leaf identity/time, not ancestor effect evaluations.
- `deadpan-app/src/worker.rs:319` resolves Source/Freeze to a qualified original
  frame but rejects accepted generated and still-image playback explicitly. Its
  Picture at `:72` has a decoded surface and optional canvas, but no framing recipe.
  `render_frame` at `:621` preserves SAR, rotation, primaries, transfer and raw PTS.
- `preview.rs:1765` first fits the project canvas into the viewer. At `:1818` it
  renders the decoded source into an integer-sized target with unconditional Fit.
  `target_size` at `:2267` floors dimensions independently, so its aspect can differ
  slightly from the authoritative canvas. Do not derive creative framing from
  this approximate preview target.
- `presentation.rs:158` decides whether to render from requested/decoded/displayed
  request identity. Camera changes on the same decoded picture would be invisible
  without a separate recipe/preview generation. Successful submission, not decode
  success or parameter mutation, must advance displayed framing identity.

## Preferred coordinate contract

Use typed coordinate spaces rather than one unlabelled center tuple. Preserve
authored exact finite values until the render geometry boundary; convert to f64
there, then explicitly checked f32 uniforms. Reject nonfinite, zero/negative
scale, overflow and unsupported cumulative transforms before GPU writes. Core
should bound both individual parameters and the cumulative nested transform.

1. **Coded plane:** edge coordinates `[0,W] x [0,H]`; texel centers are
   `(i+1/2,j+1/2)`. RGBA bytes remain unrotated and unwarped. Color/range conversion
   remains the current decoder/render contract.
2. **Upright source plane:** stretch coded X by SAR, then apply the declared
   clockwise rotation. Upright dimensions are `D=(W*SAR,H)`, swapped for 90/270.
   Normalized coordinates run from top-left `(0,0)` to bottom-right `(1,1)`, with
   X right and Y down. Camera targets and source-size key steps use this uncropped
   plane, never coded pixel axes, a rounded preview rectangle or a previous crop.
   Its center is `(1/2,1/2)`. Support membership is half-open even though target
   points on edges may validly use normalized coordinate 1.
3. **Canonical canvas plane:** the committed presentation basis `C=(Cw,Ch)`.
   A provider's baseline placement is `F(u)=C/2+k*(D*u-D/2)`, with componentwise
   D products. `k=min(Cw/Dw,Ch/Dh)` for Fit, `max(...)` for explicit Fill. Default
   Fit preserves the entire image with black uncovered canvas. Fit/Fill is the
   provider's explicit composition policy, not a behavior switched by zoom.
4. **One framing plane for every node:** source leaves and groups both retain
   explicit canvas-normalized center `g` and positive uniform scale `s`, applied
   to their child picture as `A(p)=C/2+s*(p-C*g)`. Identity is center .5/.5,
   scale 1. Apply the leaf operation, then ancestors from inner to outer. A node
   without an authored operation is identity. Do not repeatedly apply leaf Fit
   or reinterpret an ancestor center as an absolute source recenter. A 1.35 punch
   multiplies the selected operation's scale; it does not implicitly Fill.
   For a source-upright target center `c`, a leaf resolves `g=F(c)/C`; its combined
   placement is therefore `C/2+s*k*(D*u-D*c)`. Source percentages remain the input
   language while the stored framing plane stays uniform throughout the tree.
   Crossing image edges during pan/zoom-out exposes black rather than clamping
   the camera onto the image. A bounded off-canvas center can be intentional.
5. **Preview/output raster:** an output pixel center `(x+.5,y+.5)` first maps to
   canonical canvas coordinates `(Cw*(x+.5)/Tw, Ch*(y+.5)/Th)`. All framing and
   clipping run there, then inverse source rotation yields coded UV. This keeps
   semantic framing independent of target rounding, window resize or export
   resolution. Viewer chrome fitting remains outside this pipeline.

The distinction between source target/input space and canvas framing space is
explicit. A saved manual Target retains its asset, source-time identity and
source-upright point or region. Selecting it projects through the child transform
before the selected framing operation; the command authors the resulting numeric
canvas center plus target provenance. Source-axis Camera steps project through
that same pre-operation forward linear map. Never use current crop dimensions.
A saved target correction requires explicit reapplication for this manual-target
increment. It is not a live tracking link, and it does not complete the normative
tracking requirement. Future live targets need explicit source/generated
correspondence and mismatch policy rather than changing the meaning of a stored
manual center. Reject or clearly mark mismatched-source or child-clipped targets;
framing an already clipped point in an outer group cannot resurrect that image.

Counterexample requiring an explicit rule: a child pan with center .6 followed
by an ancestor identity must stay panned. An ancestor implemented as "recenter
source .5 at scale 1" would undo that pan. Likewise selecting source target .7
on a group must aim at the child's rendered location of .7, not at canvas .7.

## Nesting, coverage and providers

- Compile a bounded leaf-to-root framing program while walking the existing
  active plan path. Each evaluation retains owner, instance, local exact time,
  canvas coordinate declaration, resolved curve value and layer order. Sequence offsets,
  Repeat play-local reset, sparse overrides, and Retime local mapping are already
  available at traversal time. An effect on Repeat uses Repeat-local time across
  plays and gaps; an effect inside its child resets per play. A gap has the
  preceding stable play identity and no invented child traversal.
- Evaluate step/linear/smoothstep through one core/plan helper. The present frame
  query samples exact frame centers; specify envelope endpoints on exact frame
  boundaries rather than rounding/restarting time at a Split or seek. An effect
  on an outer Retime sees its output-local time, a child effect sees mapped child
  time. Retain only the depth-bounded active path, not expanded Repeat instances.
- For true group composition, preserve each canonical-canvas clipping boundary.
  Multiplying affine maps and dropping intermediate clips is unsound: an inner
  zoom can crop the source, and an outer zoom-out must not resurrect those pixels.
  With current positive uniform scale/translation and one provider, transformed
  clips stay axis-aligned, so their final intersection can remain one rectangle.
  Once overlays or other group effects exist, use their explicit layer/composite
  stages rather than claiming a single-provider affine map implements them.
- Keep source image support distinct from sampler tap clamping. Reject outside
  transformed source/clip support to background, then clamp the four bilinear
  taps at valid source edges. Clamping out-of-image UV instead makes stretched
  edge-colored bands. Preserve the current half-open coverage convention.
- Blank and Background remain black picture independent of camera movement.
  Root output is opaque black. Future image/layer intermediates should retain
  premultiplied linear RGBA and transparent uncovered support until composition;
  otherwise group/overlay alpha cannot be implemented faithfully. Do not expose
  transparent pixels as an invented transparent final movie. Today's opaque
  source-over-black output can remain unchanged for the single-picture path.
- Freeze holds select the same physical source frame but evaluate their own
  editorial envelope across Hold-local time. Source tracking for a freeze stays
  pinned to the frozen source identity; it must not advance with Hold time. Source
  endpoint holding similarly needs the target location of the actual held frame,
  not an out-of-range virtual source time. Keep exact query time and selected
  physical identity separate.
- Generated footage goes through its own qualified SAR/rotation/color and an
  explicit composition fit, then the Hold's editorial framing. Never bake zoom
  into the accepted artifact. A source-space target cannot be transferred to a
  differently padded/cropped generation by assertion: it needs the retained,
  qualified source-to-artifact geometry map, or an explicitly artifact-local
  manual target. Existing app rejection of accepted/still decode remains an
  implementation gap, not something this transform API should silently bypass.

## Small shared implementation boundary

No new crate is needed. `deadpan-render` already depends on core. Let core own
validated authored values/curves, plan own exact temporal and scope resolution,
and renderer own spatial projection from source metadata and canonical canvas.

Suggested shapes, subject to the core author's names:

```text
PictureSample { existing fields, framing: bounded evaluated layer list }
EvaluatedFraming { owner/scope, canvas center, scale, clip semantics }
ProviderComposition { fit policy }
PictureGeometry::with_framing(metadata, canvas, raster, evaluated_layers)
PictureGeometry::{source_uv, source_to_canvas, source_to_viewer, clip_bounds}
PictureRenderer::render_framed(frame, target, geometry)
reference_pixel_framed(frame, geometry, pixel)
```

Keep the current FitMode renderer and CPU reference functions as identity
conveniences. Derive all CPU geometry and GPU uniform coefficients from the same
validated spatial object. The CPU pixel/color calculation remains an independent
f64 numerical reference. No FFmpeg crop expression, second export framing math,
or UI-only crop implementation should appear.

The positive-scale case can extend the rectangle representation and inverse
mapping without a multipass GPU pipeline. A checked affine pair plus explicit
clip is also small and allows forward target overlays. Preserve f64 semantic
geometry for unit tests; qualify f32 uniform/raster coverage explicitly rather
than silently snapping authored centers to make one test pass.

An especially useful boundary choice is to derive the half-open integer output
coverage rectangle once in shared geometry, using `start=ceil(left-.5)` and
`end=ceil(right-.5)` in raster coordinates, clamped to the target. Pass these
bounded integer pixel limits to the shader instead of independently rejecting
coverage from rounded f32 UV. This avoids an entire black-versus-picture pixel
disagreement at a mathematically exact crop edge. Keep exact rational coverage
through this ceiling where the authored representation permits it; otherwise
declare and test the f64-to-coverage boundary rather than claiming exact f32
geometry. Sampling/color tolerance does not excuse a coverage error.

For repeated Camera previews, separate immutable source upload from geometry-only
draws. Retain one uploaded frame handle scoped to the renderer and an upload
generation; validate ownership/staleness before reuse. Keep the existing one
submission limit and latest-request behavior. Do not hash full frames or decode
again on every key, or assume the same ordinal means the same bytes across assets.
The simpler render_framed entry can land first; upload reuse is a bounded local
optimization, not justification for a second picture pipeline.

## Camera preview identity and lifecycle

Capture project session, base revision, selected owner/occurrence, entry framing
and entry image identity. Every temporary adjustment receives a checked monotonic
preview generation. Share the plan's framing evaluator while replacing the
selected layer; appending an extra final camera matrix would change nested
semantics when the selected operation is inside a group.

Add recipe/preview identity separately from the decoded source identity to the
presentation state. Render on geometry changes even when source bytes and request
ticket stay the same. Only successful GPU submission updates displayed framing
and overlay coordinates. A stale image reply cannot attach an old override to a
new project/revision. Source context remains a non-destructive Original view;
entry to destructive Camera must resolve Sequence scope explicitly.

Enter submits one revision-aware typed command and leaves the displayed temporary
result until its committed replacement is accepted. Escape removes the temporary
override and redraws the exact entry recipe with no history entry. Selection,
revision or session changes invalidate the preview. A failed commit must report
failure rather than relabel the preview as committed. Decide playback behavior
explicitly; pausing before Camera gives a stable spatial editing target.

## Proportional verification

Headless/unit and integration work should cover most behavior:

1. Independent known coordinates on an asymmetric source, all four rotations,
   non-square SAR, Fit/Fill, .01/.05 source steps, 1.05 scale and reciprocal,
   .5 identity and corner targets. Ensure visual h/l axes remain upright after
   90/270 rotation. Validate invalid/cumulative extreme scales before GPU work.
2. Same normalized canvas locations at multiple raster sizes, including preview
   sizes whose independent integer rounding changes aspect; compare mappings,
   not unlike-resolution final pixels. Include negative/off-canvas centers,
   half-open boundaries, single-pixel images and padded row strides.
3. Child pan plus ancestor identity; noncommuting child/ancestor pan/scale;
   target projection through child crop; inner crop followed by outer zoom-out
   cannot resurrect pixels. Known black/transparent-magenta edges catch UV
   clamping and non-premultiplied interpolation errors.
4. Plan fixtures for Sequence offsets, nested Retime/Repeat and sparse override,
   stable reordered play IDs, gaps, Split boundaries and start/middle/end exact
   curve samples. Assert queried time, selected picture and framing owner/order,
   not just one final matrix shared with the implementation.
5. Freeze: source-frame identity constant while Hold envelope changes. Endpoint
   hold target stays on the actual held image. Generated synthetic provider uses
   explicit aspect/fit and coordinate mapping; unsupported media still fails.
6. Presentation reducer tests: geometry-only invalidation, stale preview/decode,
   resize during Camera, failed GPU submission, Escape restoration, failed/stale
   commit, and one command/history record for many key adjustments. No native
   window needed for these identities.

Then extend `deadpan-render/examples/qualify_picture.rs` with a small deliberate
framing matrix using its existing actual offscreen Metal readback and CPU oracle.
Retain asymmetric SAR/rotation plus one nested clip/target case, black support,
transparent colored edge and awkward fractional scale/center. It already tests
2 display-code tolerance and a 0.0005 working-color anchor; keep exact expected
coverage checks separate from half-float color tolerance. Shader parse/validation
stays headless. A single bounded native Camera interaction and aesthetics check
is valuable after these pass, rather than GUI-testing every geometry combination.

No live GPU or GUI work was performed for this assessment.
