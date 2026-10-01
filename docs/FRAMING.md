# Authored framing and Camera

Static and enveloped framing are implemented, with a native temporary Camera
preview. This does not complete DP-08,
the creative-operation gate, or preview/export acceptance. The full
[specification](spec/DEADPAN_SPEC.md) remains normative. The new
[Camera board](design/boards/camera-framing-board-v1.png) is a visual target;
its saved targets and region editor are still required work.

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
| `f` | Toggle the numbered source-target picker. Digits pick targets there; they are counts in Adjust. |
| `r` | Reset the temporary framing to neutral. Later adjustments create a new static pose. |
| `Enter` | Apply one typed revision-guarded edit. Unchanged framing creates no history entry. |
| `Escape` | Discard the draft and restore the entry operation. |
| `,z` | Commit a constant 1.35× punch at the current center, replacing a prior curve. |
| `,c` | Commit a whole-beat smoothstep creep from the current evaluated pose to 1.35×, replacing a prior curve. |

Normal Camera adjustments preserve an existing envelope: all centers and controls
move by the same delta and every scale receives the same factor. The app validates
the whole candidate curve before preview or commit. It evaluates that candidate
through the same core helper used by the committed picture plan. Reset and the
two named gag commands have explicit replacement semantics.

The target picker currently provides deterministic center/corner positions. These
are upright source points projected through descendants into the selected
operation's input canvas. Clipped targets are unavailable, never silently clamped
or renamed as detected faces. Numeric center fields use **canvas percentages**;
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

## Storage and remaining work

Database 24 introduced core 18. Frozen core 17 rejects the new framing vocabulary even
when a supplied value is empty. Schema-23 migration must compare complete old
snapshots and edit history on a consistent backed-up copy before promotion, with
existing operational media/generation records preserved. Framing-only authoring
does not by itself change source media, sound, time or generation conditioning.
Database 25 now stores core 19 and freezes core 18 before admitting captured Hold
geometry. See [the capture contract](CAPTURED_FRAMING.md) for its stricter legacy
recipe boundary and migration fixture.

Saved named manual point/region targets, keyboard region creation, actual detection
and source-time tracking, per-play crop escalation, nested native selection, full
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
