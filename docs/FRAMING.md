# Authored framing and Camera

Static, enveloped and followed framing are implemented, with a native temporary
Camera preview that also picks, draws, follows, corrects and tracks
[attention targets](TARGETS.md). This does not complete DP-08,
the creative-operation gate, or preview/export acceptance. The full
[specification](spec/DEADPAN_SPEC.md) remains normative. The
[Camera board](design/boards/camera-framing-board-v1.png) is a visual target;
its `Save as` name field and target-picker dropdown are not implemented
(Camera names new targets `Target N`).

## Authored operation

Core document 18 adds optional `BeatNode.framing` and typed `SetFraming` commands,
including occurrence isolation. Framing has a static pose or an explicit envelope
of at most 64 segments. Poses store exact canvas-normalized center coordinates
and uniform scale. Supported curves are Step, Linear, Smoothstep and cubic Bezier
with explicit pose controls. There are no hidden animation names or tracking
links. Core validates centers in `[-16,17]`, scale in `[1/64,64]`, aggregate record
limits, and at most 16 authored framing operations on a structural path.

An `OwnerOutput` envelope uses its owner's complete output duration. A picture sample evaluates
the exact owner-local frame-center coordinate before descending through the
structure. A curve inside Repeat resets per play; one on Repeat spans plays and
gaps. Retime framing sees its output clock; child framing sees mapped input time.
Changing the owner's duration intentionally stretches its whole-host envelope.
Core 38 also supports an explicit retained duration and local offset for physical
Source growth. That clock holds endpoint poses in new handles and preserves the
existing path; see [Source effect clocks](SOURCE_EFFECT_CLOCKS.md).
An 11-frame creep stores endpoints at boundaries 0 and 11; its first and last
picture samples are at 0.5 and 10.5, not at those endpoints.

Timeline and segment selection remain exact. Interior numeric interpolation uses
the declared Q32 round-even grid. Static values and exact segment endpoints retain
their authored ratios. This numeric precision is not a rounded project/source
clock. Cubic pose controls use ordinary Bezier value interpolation with linear
segment progress. Keyframe time handles and arbitrary fixed-time envelopes remain
separate required work.

`Framing::evaluate_exact` also accepts an exact positive derived owner extent.
The integer `evaluate` API delegates to it. Segment selection compares products
without first constructing `local / duration` or `duration * endpoint`, which can
exceed `ExactRatio` even when every input is valid. Five fixed 64-bit limbs cover
the largest 294-bit product and the one-bit shift used by Q32 division. There is
no floating-point time calculation or arbitrary-precision runtime dependency.
Independent Fraction fixtures cover rational extents, tiny and large domains,
half-even ties, and products beyond the ordinary ratio representation.

This is numerical support for the [derived-clock splice design](STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks).
Current documents and render plans still expose integer owner durations. No
fractional structural extent, new command or schema is admitted by this API.
The [qualification record](qualification/framing-clocks-2026-09-26.md) records
the reference cases, independent numerical review and repository checks.

Split retains complete effect owners behind transparent partitions. Re-splitting
a framed partition must retain that meaningful wrapper instead of discarding its
framing. Copies and occurrence isolation copy owned numeric curves independently.
Ungroup rejects a framed Sequence until equivalent effect distribution and group
clipping are implemented. It must never silently remove an authored camera path.

## Shared picture geometry

The renderer interprets source SAR and rotation before composing the upright
image into the canonical project canvas with explicit Fit/Fill. The app uses Fit.
Zoom does not implicitly change that policy. Each framing operation maps incoming
canvas position `p` to:

```text
q = canvas / 2 + scale * (p - canvas * center)
```

The identity is center `(0.5,0.5)`, scale 1. Operations run from provider to root.
The provider's own operation runs before its first canvas clip; ancestor operations
run after it and retain each intermediate clip. An outer zoom-out cannot recover
pixels a child crop already removed. A Repeat gap has a separate provider before
the Repeat's operation. Black remains black outside picture support; sampling
clamps filter taps only after coverage has been established.

Creative coordinates use the canonical canvas, not the rounded preview texture.
Resizing the window does not change framing. Canvas edits reevaluate the same
normalized operation and source Fit under the new aspect without changing time.
They do not promise to keep a previously chosen source point centered. That needs
explicit source-target reapplication or an implemented live target relationship.

`PictureSample.framing` retains provider-to-root scope identities, exact local
positions, owner durations and evaluated optional poses. The shared
`PictureGeometry::composed` and `PictureRenderer::render_composed` apply optional
[captured Hold composition](CAPTURED_FRAMING.md) before those live operations.
The existing `framed` / `render_framed` entrypoints use no captured context.
Both paths serve native preview and offline callers. Integer pixel coverage is derived before f32 GPU sampling;
the CPU reference retains f64 spatial/color calculations. An actual encoded export
consumer, physical display qualification, HDR and all remaining picture effects
remain open.

## Native interaction

Native framing targets a selected direct child of the current ordinary Sequence
in Your edit and a successfully
displayed stopped picture. The immutable Original view remains a browsing context.
Camera entry pauses audition and waits for that exact picture. Revision, session,
selection, navigation scope and cursor changes revoke the draft.

| Input | Behavior |
| --- | --- |
| `,f` | Enter Camera with current framing unchanged. |
| `h/j/k/l` | Move the center by 1% of the upright uncropped source width/height. |
| `H/J/K/L` | Move by 5% of the same source dimensions. |
| `+` / `-` | Multiply scale by 1.05 or its reciprocal. Counts repeat the operation. |
| `f` | Toggle the numbered picker: saved targets covering this picture, then the center and corners. Digits pick there; they are counts in Adjust. |
| `t` | Follow the picked saved target (`FramingValue::Follow`, the current scale, fallback = the current pose); `t` again stops following at the pose shown. |
| `n` | Draw a new target rectangle (below). |
| `c` | Correct the picked or followed target at this picture (below). |
| `T` | Track the picked or followed target in the background ([tracking](TRACKING.md#in-the-app)). |
| `r` | Reset the temporary framing to neutral. Later adjustments create a new static pose; a follow stops. |
| `Enter` | Apply one typed revision-guarded edit. Unchanged framing creates no history entry. |
| `Escape` | Discard the draft and restore the entry operation. |
| `,z` | Punch in to 1.35× on the selected target ([below](#zoom-and-creep-commands)). |
| `,c` | Creep from the current evaluated pose to 1.35× over the beat or the Edit range inside it. |

Normal Camera adjustments preserve an existing envelope: all centers and controls
move by the same delta and every scale receives the same factor. The app validates
the whole candidate curve before preview or commit. It evaluates that candidate
through the same core helper used by the committed picture plan. Reset and the
two named gag commands have explicit replacement semantics.

The picker lists, in the order of specification section 7.6, saved targets whose
asset is the displayed picture's and whose span covers its source time, then the
center and four corners of the upright Original. Digits reach the first nine
entries; later entries (the last corners first) have no number and are listed
in the inspector only. Saved targets are drawn as
rectangles at `region_at` of the picture's exact source time, colored and labelled
by how that region was obtained: `drawn` (the initial rectangle), `corrected`,
`tracked`, `interpolated` or `lost · holding`. All entries are upright source
points projected through descendants into the selected operation's input canvas.
Clipped targets are unavailable, never silently clamped or renamed as detected
faces. Picking a saved target centers the draft on it statically and selects it
for `t`, `c` and `T`.

### Following a target

`t` turns the draft into `Follow { target, scale, fallback }` with the draft's
scale and the current pose as fallback. Camera also opens on an existing follow:
the target supplies the center and `+`/`-` (or the Scale field) change the follow
scale; the fallback keeps its exact center and scales by the same factor.
Center nudges (`h/j/k/l`, center fields, a fixed picker point) are refused with
a message while following; the center fields are disabled. `t` stops following
and keeps the pose on screen, which Enter saves as a static pose. Picking another
saved target while following switches the follow's target, an explicit target
change rather than a hidden crop coordinate. Following an enveloped beat replaces
its curve only on this explicit request, and the inspector says so.

The preview resolves the follow exactly as the committed plan does
(`deadpan_plan::follow_pose`: the target's center at the picture's source time,
carried through every inner operation), then re-resolves every outer follow that
sees the changed operation, and replaces those poses atomically on the retained
picture. Escape restores every operation's entry pose. The `targets` replay
checks that the committed picture's pose equals the Camera preview exactly.

### Keyboard target rectangles

`n` opens a rectangle centered on what the viewer shows, a quarter of the
visible source in each dimension. `c` opens the target's rectangle at this
picture. Tab moves between the Center, Width and Height fields; arrows or
`h/j/k/l` change the focused part by 1% of the upright source, Shift by 5%, and
counts repeat. Width and Height grow with Right/Up and shrink with Left/Down,
never below 1%; centers stay inside the picture. Enter saves; Escape returns to
framing without an edit. The mouse is not required and not yet supported.

A new target is one undoable `SetTarget` with a fresh `target-N` id and
`Target N` label. Its span starts at the displayed picture's indexed PTS (the
rectangle is the region selected there, as the core model requires) and ends at
the next stored shot boundary of the Original, or at the end of its measured
video when no shot analysis is stored. A correction of an untracked target
replaces its initial rectangle at the span start or stores a manual correction
at this picture; a correction of a tracked target re-tracks only its range in
the background.

Target saves keep Camera open: a revision created by this Camera's own target
save (matched by session, base revision and saved revision) rebuilds the session
from the new picture and carries the draft pose, reset, follow and picked target
over. Any other revision change still revokes the draft. Numeric center fields use **canvas percentages**;
movement keys use **uncropped source percentages**. These differ when aspect or
descendant framing differs. Derived spatial values quantize once at the declared
Q32 numeric boundary.

Camera updates the retained decoded picture's evaluated operation without new
source decoding. A separate monotonic geometry revision makes a redraw necessary
even when the physical source frame stays unchanged. Only successful GPU submission
advances displayed geometry. A new request immediately revokes old Camera write
tickets. Enter retains the draft picture until the committed replacement is ready;
a failed/stale commit cannot label the draft saved. GPU input uploads still occur
on each framed submission; upload reuse and measured latency remain open.

## Zoom and creep commands

`,z`, `,c`, `:zoom` and `:creep` build one typed `SetFraming` edit for the
selected direct child (or the inspected Repeat play) after its stopped picture
is displayed, through the same Camera entry checks, macro recording and
revision guard as Camera. The construction is pure and unit-tested in
[`navigation/zoom.rs`](../crates/deadpan-app/src/navigation/zoom.rs).

| Input | Result |
| --- | --- |
| `,z` | `:zoom 1.35 target=current`, except that with no saved target it punches in at the current center and says so. |
| `,c` | `:creep to=1.35`. |
| `:zoom S [target=…] [curve=step]` | Smash zoom. On the whole beat with a target: `Follow { target, scale: S, fallback }`, a live step onto the target. Otherwise a static pose at scale `S`. |
| `:zoom S curve=linear\|smoothstep` | An eased change from the current pose, as `:creep to=S`. |
| `:creep [from=S] [to=S] [target=…] [curve=smoothstep\|linear]` | An envelope from the current center at `from` (default: the current scale) to `to` (default 1.35). |
| `:zoom off` | Abrupt return: no framing on the whole beat, or a step out to the full picture over the Edit range. |

`target=current` (and `,z`) means the target the beat already follows, else the
only saved target covering the displayed picture; several refuse and name them.
`target=center` is the Original's center; any other value is a target id or
label (quote labels with spaces). A whole-beat step with a target follows it
live. An eased or ranged edit uses a fixed point: the target's center exactly
as a follow would resolve it (`deadpan_plan::follow_pose`) at the frame where a
step lands (the range's first frame) or where a creep arrives (its last frame).
The status line names that frame and says the framing does not follow. A target
not visible at that frame refuses.

Nothing flattens an existing path implicitly. On the whole beat:

- a beat that follows a target keeps following when `:zoom S` or `,z` changes
  only its scale (the fallback keeps its center; the clock is kept); a creep
  without `target=` refuses, because a fixed center would stop the follow;
- a camera path (envelope) refuses `,z`, `,c`, `:zoom` and `:creep` unless the
  command names `target=` explicitly, which replaces the path on request;
  `:zoom off` always removes framing.

With an Edit range inside the beat, only that range changes: Step segments hold
the beat's static pose before it, the range gets the new pose, and a step
returns to the previous pose after it. A ranged creep arrives at the range end
and then holds its end pose to the end of the beat, which the status line says.
A ranged edit needs a beat with no framing or one static pose on any clock (a
static pose is constant); a follow or camera path refuses with its own reason.
A range outside the beat refuses; a Repeat play is always framed whole.
Envelope progress is exact (`frame / beat frames`, reduced, at most one million).

`:zoom` and `:creep` capture the session, revision, view, selected beat, Repeat
scope, cursor and Edit range when `:` opens, including their absence; any
change before Enter refuses instead of retargeting. Macros record whole-beat
results as `SetFraming`; ranged results are refused while recording, because
an envelope in beat fractions would not mean the same range on another beat.

The save message describes the committed framing (`Framing saved: 1.350×
following Target 1`), and the inspector shows `Path · 1.00–1.35×` for camera
paths. The `zoom` replay checks the center fallback and its message, the
target follow and its displayed pose, `:zoom … target=center`, `:creep … target=current`,
refusal to flatten a path, `:zoom off`, a ranged punch-in at exact frames,
refusal of `,c` on a path, black punctuation, a recorded `:zoom` replayed after
Undo, and the two-target refusal of `,z` followed by a quoted-label `:zoom`.

## Framing presets

`:framing-save a` keeps the selected beat's framing (an off-center stare from
Camera, a zoom, a creep or a follow) as a one-instruction macro
`SetFraming { framing }` in register `a`. Select another beat and press `@a` or
`:macro a` to apply it as one Undo; it replaces that beat's framing exactly as a
recorded `,z` or `,c` does. Envelopes are in owner progress, so a creep preset
spans the whole new beat; a follow preset needs its target. Presets live in the
persisted register bank and list in `:registers` as macros; there is no
separate named preset store yet.

## Storage and remaining work

Database 24 introduced core 18. Frozen core 17 rejects the new framing vocabulary even
when a supplied value is empty. Schema-23 migration must compare complete old
snapshots and edit history on a consistent backed-up copy before promotion, with
existing operational media/generation records preserved. Framing-only authoring
does not by itself change source media, sound, time or generation conditioning.
Database 25 now stores core 19 and freezes core 18 before admitting captured Hold
geometry. See [the capture contract](CAPTURED_FRAMING.md) for its stricter legacy
recipe boundary and migration fixture.

Saved region targets, keyboard region creation, following and selected-target
tracking are implemented (above, [targets](TARGETS.md), [tracking](TRACKING.md)).
Point targets, face/region detection, renaming targets, pointer dragging of
rectangles, a live follow whose scale changes over time (a creep that tracks a
moving target), and follow centers corrected for a letterboxed canvas remain
required. Centered per-play scale
escalation is in [Repeat escalation](REPEAT_ESCALATION.md); target-centered escalation, nested native selection, full
framed Ungroup, arbitrary temporal envelopes, captions, cutaways, and the remaining
Section 8 picture operations remain required. [Captured framing](CAPTURED_FRAMING.md)
retains the cropped composition for native Source/Freeze pause insertion, separately
from the exact source PTS and new Hold framing. Root framing remains inherited
once; lower curves become their sampled poses. Camera reset keeps the captured
crop. Arbitrary nested insertion and capture from still or accepted footage
remain required.

The [qualification record](qualification/framing-2026-09-24.md) separates the
repository gate, actual Metal/CPU comparisons, old-binary migration fixture,
independent code review and bounded native aesthetics/keyboard review. Headless
tests cover exact scope/time mapping, structural preservation, stale/cancel/failure
transitions, controlled numeric fields and durable history. Native checks do not
qualify VoiceOver, physical non-US layouts, CJK composition, performance or export.
