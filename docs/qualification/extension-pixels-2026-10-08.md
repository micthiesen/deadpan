# Extension pixel rejection checks, 2026-10-08

One-sided extensions now have host motion, lighting and actual-join pixel
checks. They inspect private canonical media and the pre-launch retained PNGs.
This advances §12.5; it does not enable extension Ready or acceptance. Directional
face, mouth and selected-region checks and full provenance admission remain.

## Generated interval and exact joins

`ExtensionMotionReport` inspects every adjacent generated-native pair, including
pictures that downsampling omits. Retained native context is outside this
interval and does not count as generated motion. Both directions preserve
chronological native ordinals. The exact `FrameCentersClamped` mapping gives
pair spacing `N / (project_fps × E)`, where `N` is the authored frame count and
`E` the generated-native count. This stays defined for a one-frame output;
its internal native coverage does not imply sampled-output motion coverage.

`ExtensionEndpointReport` reads the actual sampled first and last pictures.
For a one-frame output it compares that same sampled middle picture at both
present joins. FromLeft conditions the entry and FromRight the exit. The other
join is explicitly unconditioned when present, or absent with no measurements.
Every present join uses its retained input PNG, never the worker's native context
output. It measures broad RGB discontinuity and motion/lighting within the
captured presentation rectangle, at one project frame of spacing.

The existing Bridge numeric thresholds and pixel kernels are shared unchanged.
Gross discontinuity can reject an unconditioned join; a passing check does not
turn that join into a conditioned return. Low-texture motion remains unavailable,
independently of measured RGB or lighting. A measured rejection returns its
profile, join or native ordinal and values.

## Binding and bounds

`inspect_extension_pixels` accepts one private `CanonicalExtension` and the
separately retained inputs. It checks the immutable request and exact sampling
map before decoding. Reports retain plan, object, input receipt, crop, policy
and complete observation identities. Validation repeats coverage and rejection
rules; edited thresholds, roles, clocks, objects or omitted pairs cannot validate.
Both private conversion reports must also match the full native and sampled
video contracts, including raster dimensions that the sampling map does not carry.
This is observation validation, not a new decode or store admission authority.

The generated-pair report has a 1,024-pair parse/inspection bound and indexes at
most 4,096 native pictures. Sampled inspection retains the existing 65,536-frame
limit and verifies every indexed PTS before inspecting at most two pictures.
Readers use verified private descriptors, bounded raster/decode work and one
caller-owned deadline. Cancellation and deadline checks surround preparation,
decode and comparison. Retained PNG readers return to their start on success
or failure. Extension unavailable-motion and absent-join tags reject hidden
fields. Bridge serialization and existing pixel behavior are preserved.

The development `qualify_extension_media` example now requires the first tool's
host-owned pre-launch receipt and retained input directory. It rechecks those
bytes, canonicalizes output and inspects pixels under a shared deadline. Its
report includes the observations and clearly lists the remaining admission
checks. It cannot create a Ready receipt or edit a project.

## Verification

Evidence is retained in
[`tools/model-qualification/evidence/2026-10-08-extension-pixels`](../../tools/model-qualification/evidence/2026-10-08-extension-pixels/).
The first corrected focused run passed all 33 selected tests. The workspace gate
passed formatting and strict workspace/UI Clippy, then passed 5,295 of 5,296
tests (10 skipped). Its one failure was an unrelated real HDR mux duration
defect: the retained 46-frame output declares 47 ticks in video `mdhd`, while
`stts` and the actual pictures span 46. The failed movie and manifest are in
[`2026-10-08-mux-duration`](../../tools/media-qualification/evidence/2026-10-08-mux-duration/).
An isolated HDR rerun passed; that does not fix the retained failure. The store
test reported as leaky by the full run passed
in isolation without a leak report. No store assertion failed.

After the diagnostic-clock correction, all 195 model/media-worker tests passed
(one ignored). One worker-boundary test had a pipe-closure leak diagnostic; it
passed without that diagnostic in the later focused run. The completed raster
binding and mux corrections passed all 119 focused tests. The fresh full
workspace run then passed all 5,307 tests, with 10 intentional skips and no
leak reports. All 1,071 UI-harness tests passed (two intentional skips, one
pipe-closure leak diagnostic), as did both doctests, formatting and strict
workspace/UI Clippy. Initial and final source identities and logs are retained.

The final UI pipe diagnostic names
`nested_macro_copies_keep_their_staged_absolute_bounds_and_original_group_labels`.
Read-only tracing found threads and in-memory/editor storage work but no child
launch in that test. No surviving pipe writer was identified. Runner descriptor
inheritance is a hypothesis, not an established cause; the warning is retained
without claiming that a passing assertion or another run resolves it.

Real decoder/converter fixtures exercise both directions, N=1 and N=8, present
and absent opposite seams, changed worker-side input files, violent context-only
changes, flashes omitted by downsampling, slow ramps with bad far joins and a
sampled middle that fails despite matching native endpoints. They use genuine
black PNGs for authored-black inputs. Report mutation, stale media/sampling,
cancellation and expired deadlines are also checked. These are synthetic
pixel/timing oracles, not model-quality or face-identity evidence.

The first focused run found two fixture assumptions: the clock mutation omitted
the externally tagged `authored_black` field, and the timing test assumed Bridge
and extension formulas never coincide (they do for its K=9, E=8, N=1 case).
Both were corrected without changing production timing or rejection rules.
Independent review found a missing conversion-limit check before developer
deadline arithmetic; the example now validates limits first. Reciprocal review
also found that retained validation needed to bind the raster in both private
conversion reports. The full-contract check and a same-sampling/different-raster
regression cover that gap. Sampled-join diagnostics now name the sampled clock;
the shared Bridge path retains its native-clock diagnostics.

## Remaining work

Add a true single-anchor face/region policy and directional tracking. FromRight
must follow the anchor outward in reverse chronological order while retaining
canonical ordinals; mouth checks must cover only generated pictures in time
order. The unconditioned opposite PNG must never become a synthetic expected
target. Then complete provenance, durable Ready/acceptance, native controls and
the longer measured model envelope. No model inference, packaged app, native
UI or export qualification is claimed here; DP-12 remains Partial.
