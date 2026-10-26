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
| `f` | Toggle the numbered picker: saved targets covering this picture, center and corners, then detected faces and salient regions. Digits pick there; they are counts in Adjust. |
| `Tab` / `Shift+Tab` in the picker | Next / previous page of up to nine numbered targets. |
| `t` | Follow the picked saved target (`FramingValue::Follow`, the current scale, fallback = the current pose); `t` again stops following at the pose shown. |
| `n` | Draw a new target; `n` in the editor switches rectangle / point (below). |
| `c` | Correct the picked or followed target at this picture (below). |
| `T` | Track the picked or followed rectangle in the background; points have no tracking box ([tracking](TRACKING.md#in-the-app)). |
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

The picker lists saved targets whose
asset is the displayed picture's and whose span covers its source time, then the
center and four corners of the upright Original, then detected faces and salient
regions. Each page uses digits 1–9; Tab and Shift+Tab reach every page. Background proposals
append without changing existing entry positions. Saved targets are drawn as
rectangles or point markers at `region_at` of the picture's exact source time,
colored and labelled by how that geometry was obtained: `drawn` (the initial selection), `corrected`,
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
Center nudges (`h/j/k/l`, a fixed picker point) are refused with
a message while following; the target-owned center is shown as plain read-only
canvas percentages. `t` stops following
and keeps the pose on screen, which Enter saves as a static pose. Picking another
saved target while following switches the follow's target, an explicit target
change rather than a hidden crop coordinate. Following an enveloped beat replaces
its curve only on this explicit request, and the inspector says so.

Follow normally centers freely on the projected target. An exploded targeted
Repeat carries an explicit `CanvasClamped` policy instead: above 1×, the center
is bounded so the view stays inside its layer's canvas, including letterbox
bars. At 1× or below it remains free. Camera and the inspector explain this
policy; scale/arrival/curve changes and target replacement retain it. An absent
or hidden target keeps its authored fallback without applying a live-target
clamp. The default unconstrained policy is omitted from serialized framing.

An animated Follow also exposes Arrival scale (%) and Arrival curve. Tab reaches
Scale, Arrival, then Curve; Enter or Space on Curve switches Linear/Smoothstep,
and Up/Down adjusts the focused numeric field or switches the curve. Arrival
changes only the endpoint, while Scale still adjusts the whole animation.
Untouched exact values, its timing window and fallback center remain intact.
Valid typing previews immediately without rewriting an unfinished decimal;
invalid text or active IME composition blocks Apply. Enter finishes a text field,
then Enter in the viewer or Apply commits one edit. Escape restores the entry.

Editing a child in Camera also reprojects outer Follow and targeted escalation
layers through the temporary crop. Apply uses those same committed-plan rules;
Escape restores both the authored framing and escalation views captured on entry.

The preview resolves the follow exactly as the committed plan does
(`deadpan_plan::follow_pose`: the target's center at the picture's source time,
placed on the presentation canvas where the `Fit` layout puts it when the target
records its display aspect, carried through every inner operation; see
[targets](TARGETS.md#following-a-target)), then re-resolves every outer follow that
sees the changed operation, and replaces those poses atomically on the retained
picture. Escape restores every operation's entry pose. The `targets` replay
checks that the committed picture's pose equals the Camera preview exactly.

### Keyboard point and rectangle targets

`n` opens a rectangle centered on what the viewer shows, a quarter of the
visible source in each dimension. Another `n` switches to a point, with its
center alone; switching back remembers the temporary rectangle size. `c` opens
the target at this picture and keeps its saved geometry kind. Tab moves between
the Center, Width and Height fields for a rectangle and stays on Center for a
point; arrows or
`h/j/k/l` change the focused part by 1% of the upright source, Shift by 5%, and
counts repeat. Width and Height grow with Right/Up and shrink with Left/Down,
never below 1%; centers stay inside the picture. Enter saves; Escape returns to
framing without an edit. Points draw a cross and center marker, never an
invented bounding box.

The same draft also supports pointer input: drag visible picture outside the
rectangle to draw, its body to move, and any corner to resize against the
opposite corner. Points can be moved by dragging their marker or placed by
starting elsewhere on the visible picture. Pointer coordinates pass through
the actual retained raster and every captured crop and framing layer into the
upright Original. Bars and clipped source areas cannot supply coordinates.
Release keeps the draft unsaved; Enter is still the save action. Keyboard
input, focus loss or a changed picture/raster revokes a held drag. See
[pointer qualification](qualification/pointer-targets-2026-10-25.md).

A new target is one undoable `SetTarget` with a fresh `target-N` id and
`Target N` label. Its span starts at the displayed picture's indexed PTS (the
geometry is the point or region selected there, as the core model requires) and ends at
the next stored shot boundary of the Original, or at the end of its measured
video when no shot analysis is stored. A point correction replaces its initial
center at the span start or saves a manual correction at a later picture,
invalidating samples only until the next correction or span end. Tracking a
point refuses before work or writes. An untracked rectangle can be corrected
at its span start; later corrections require tracking first. A correction of a
tracked rectangle re-tracks only its range in the background.

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
revision guard as Camera. Parsing stays in
[`navigation/zoom.rs`](../crates/deadpan-app/src/navigation/zoom.rs); pure framing
construction and its numeric tests live in
[`framing_zoom`](../crates/deadpan-core/src/framing_zoom.rs).

| Input | Result |
| --- | --- |
| `,z` | `:zoom 1.35 target=current`, except that with no saved target it punches in at the current center and says so. |
| `,c` | `:creep to=1.35`. |
| `:zoom S [target=…] [curve=step]` | Smash zoom. On the whole beat with a target: `Follow { target, scale: S, fallback }`, a live step onto the target. Otherwise a static pose at scale `S`. |
| `:zoom S curve=linear\|smoothstep` | An eased change from the current pose, as `:creep to=S`. |
| `:creep [from=S] [to=S] [target=…] [curve=smoothstep\|linear]` | Eases from `from` (default: current scale) to `to` (default 1.35). With no `target=`, a Source, Hold, Sequence, Repeat or Retime with a fixed Follow retains live targeting and animates scale. Ordinary owners and explicit single-Play scopes support whole owners or contained Edit ranges. Other cases use a fixed-center envelope. |
| `:zoom S target=face:N …`, `:creep … target=face:N` | Face `N` (1 to 64, left to right) proposed in the displayed picture, saved as a target together with the framing ([face proposals](#face-proposals)). |
| `:zoom S target=region:N …`, `:creep … target=region:N` | Salient region `N` (1 to 64, geometrically ordered) proposed in the displayed picture, saved as a target together with the framing. |
| `:zoom off` | Abrupt return: no framing on the whole beat, or a step out to the full picture over the Edit range. |

`target=current` (and `,z`) means the target the beat already follows, else the
only saved target covering the displayed picture; several refuse and name them.
`target=center` is the Original's center; `target=face:N` is a face proposal
and `target=region:N` a salient-region proposal (below); any other value is a target id or label (quote labels with spaces). A whole-beat step with a target follows it
live. An eased edit that explicitly names a target uses a fixed point: the target's center exactly
as a follow would resolve it (`deadpan_plan::follow_pose_in_context`) at the frame where a
step lands (the range's first frame) or where a creep arrives (its last frame).
The status line names that frame and says the framing does not follow. A target
not visible at that frame refuses.

Nothing flattens an existing path implicitly. On the whole beat:

- a fixed Follow keeps following when `:zoom S` or `,z` changes only its scale
  (the fallback keeps its center; the clock is kept);
- a whole Source, Hold, Sequence, Repeat or Retime with a fixed Follow accepts `,c` or `:creep` without
  `target=`. Its center keeps following the target at each picture's Original
  time while scale changes on that owner's output clock. A frozen Hold keeps
  its exact Original moment while scale progresses over the Hold's duration.
  A Sequence follows each child's current Original picture while scale progresses
  continuously over the complete group. Select the group from its parent
  Sequence; this does not add root-Sequence selection or occurrence authority.
  Child framing, sound and duration remain unchanged.
  Select a Repeat from outside it to frame all plays and intervening gaps with
  one output clock. The target follows each play or gap's actual Original point;
  per-play escalation stays a separate inner layer, including its owning gap.
  No time is added after the last play.
  A Retime uses its complete output duration for scale and the exact mapped
  Original point for the target, including fractional coordinates rather than
  the rounded decoded picture ordinal. Its child framing and pitch policy stay
  unchanged.
  An ordinary contained Edit range changes only the scale transition window:
  the original live and fallback scales remain before In, `from=` takes effect
  exactly at In, scale reaches `to=` at Out and holds afterward, and the target
  remains live throughout. With no `from=`, the ramp starts at the original live
  scale. The fallback uses the same proportional change from its own base;
  neither branch takes its center from the stopped picture. A full-length Visual
  range retains its explicit range intent.
  Within a Repeat, choose This play at every enclosing Repeat level. A root
  Edit Time selection may retain an exactly contained transition window;
  without a range the whole owner is framed. The existing scoped edit
  isolates only those concrete occurrences, retaining stable play identities and
  the other plays' pictures, gap recipes and audio. Default, mixed, missing or
  mismatched Play scopes refuse. The selected owner's local output clock
  controls scale even when enclosing Retimes map it to a different project clock;
- an animated Follow accepts `:creep to=S curve=linear|smoothstep` without
  `target=` after clearing the Edit range. This retunes its arrival scale and
  curve while retaining the target, fallback center, authored clock and existing
  ramp window. Omitting `from=` preserves the starting scales. For a ranged
  animation, an explicit `from=` changes its ramp start while retaining the
  prefix; for a whole animation it rebases both live and fallback scales
  proportionally. A new Visual range refuses instead of moving the old window.
  Camera's Arrival scale and Curve controls edit these independently in its
  temporary preview; Enter saves once and Escape restores the complete entry;
- a camera path (envelope, or an animated Follow outside that retuning case) refuses `,z`, `,c`, `:zoom` and `:creep` unless the
  command names `target=` explicitly, which replaces the path on request;
  `:zoom off` always removes framing.

Animated Follow stores one optional `scale_transition` with a terminal `factor`
and `linear`, `smoothstep` or raw `step` curve. The live scale and fallback scale
retain separate initial values; each terminal value is its own initial scale
times the factor. An explicit `from=` proportionally adjusts both bases before
creating the transition. A new transition uses OwnerOutput over the current exact
owner extent, even if its fixed Follow previously had a retained clock. Its
window is normalized to that new framing duration; later structural edits retain
the authored normalization clock. Interpolation uses the existing Q32 round-even
numeric contract. Fallback centers and exact endpoint
scales stay exact; resolved target poses use the existing whole-pose grid.
An optional `range` stores exact normalized `start` and `end` plus `from_factor`
relative to each unchanged base. Its endpoints use the same denominator bound
of one million as ordinary framing envelopes. Outside the window the target
continues to resolve against each picture's Original time. Later structural
edits retain the authored window through the existing normalization clock.
Raw validated framing supports the transition on every supported owner type.

Lost tracking holds the last retained region while scale continues changing.
Outside the target's span, or when existing target projection cannot resolve,
the fallback center applies and its own scale continues changing. A captured
Hold projects the target through its retained Fit/Fill canvases and every
ordered clip, then through current live framing. Clipped targets cannot be
restored by a later zoom-out. Every live child scope also clips, even without
captured geometry: a point hidden by a child crop uses the fallback rather than
recentering missing pixels. This corrects the earlier context-free projection
that retained exterior centers. Legacy targets without a measured display aspect
use the fallback under captured geometry. Provider, crop and sound recipes stay
unchanged.

Camera Enter without adjustments retains the full authored Follow. Scale
adjustments proportionally change both bases while retaining the factor and
clock, and refuse if either endpoint would be invalid. `t` explicitly releases
the target at the installed picture pose and removes the scale animation;
Enter saves that static framing, and Escape keeps the entry framing. The
inspector and Camera show both scale endpoints and the curve. The scoped
inspector separates Follows, Scale before, Creep, Window and Framing clock;
its window names percentages of framing duration, and a retained clock also
shows its exact duration and offset. These rows describe the saved recipe.

With an Edit range inside the beat, only that range changes: Step segments hold
the beat's static pose before it, the range gets the new pose, and a step
returns to the previous pose after it. A ranged creep arrives at the range end
and then holds its end pose to the end of the beat, which the status line says.
A ranged edit needs a beat with no framing or one static pose on any clock (a
static pose is constant), except that implicit Keep-center creep also accepts
one fixed Follow as described above. Ranged step/off and explicitly sampled
target edits on a Follow, or any ranged edit on an existing camera path, refuse
with their own reason.

For an abrupt return from an existing path at a chosen boundary, move there and
press `s`. Enter the selected right fragment to reach its copied original
framing owner, then use Camera `,f`, `r`, Enter. Reset is temporary until Apply;
Escape keeps the path. Backspace returns to the fragment. One Undo restores
the framing; the next restores the unsplit owner. The earlier side keeps its
exact animation and clock. Ancestor framing stays live, so this returns to the
view entering that owner. Clearing the unframed outer Partition itself cannot
remove a child's path. The [native and emitted-file qualification](qualification/abrupt-return-2026-10-25.md)
covers retained Envelope and animated Follow paths, complete audio and history.

A range outside the selected owner refuses. Inside a Repeat, choose This play
at every enclosing Repeat, then finish `v` / frame motion / `v` before entering
the framing command. Endpoints stay on the root Edit clock. The plan maps the
complete interval through enclosing Retimes and crops to the owner's exact output clock;
it never subtracts a rounded visible-frame start. A frame only partly inside a
fractional occurrence boundary refuses, even when its center samples the owner.
The stopped cursor must show that owner. Empty, cross-occurrence, All/mixed or
several-play selections and a plain nested Retime without Play authority refuse.
The scoped selection is framing-only: Camera and other owner parameters refuse
until Escape clears it; read-only audition and independent sound controls remain
available. The first Escape clears the range before leaving the inspector.
Slice placement and replacement require the parent even after the range is
cleared; their pointer controls follow the same scope rule as their keys.
New scoped ranged target sampling refuses; Keep and OriginalCenter remain
available when the static baseline permits them, while a fixed Follow admits
only implicit Keep creep. Existing whole-owner paths keep their previous behavior.
Envelope progress is exact (`frame / beat frames`, reduced, at most one million).

`:zoom` and `:creep` capture the session, revision, view, selected beat, Repeat
scope, cursor and Edit range when `:` opens, including their absence; any
change before Enter refuses instead of retargeting. Whole-owner Keep-center
Follow creep records semantic `Zoom` intent, so named replay and dot resolve the
invocation's target, both scale bases and duration. Other ordinary whole-beat
macro presets retain their `SetFraming` behavior. Ordinary ranged commands record `Zoom`
intent with scales, curve and `Keep` or explicit `OriginalCenter`; replay
resolves the invocation's selected owner and exact range against each staged
document. It retains no old range fractions or sampled target point. A
successful ranged Apply supplies the same invocation-relative intent for `.`.
Recording a range that samples a selected, current, named or face target
refuses. With This play chosen at every enclosing Repeat, whole-owner or
supported ranged framing uses the same Zoom intent for recording and dot.
Each invocation captures its
own exact play, maps authoring and presentation addresses through staged
isolation, and returns a scoped receipt. Only Zoom and named framing calls are
admitted with a scoped Time range; All/mixed/multiple plays and sampled-target
recording refuse. Select and finish the range before `q`: scoped selection
gestures and navigation remain unrecordable. Named and dot replay resolve a fresh
range in the newly selected play, including its current crop and exact output duration.
See [one-play framing macros](SEMANTIC_MACROS.md#one-play-zoom-and-creep).

The native and semantic paths share the pure
[`framing_zoom` builder](../crates/deadpan-core/src/framing_zoom.rs) and its
[numeric tests](../crates/deadpan-core/src/framing_zoom/tests.rs). The shared
host resolver uses the staged render plan's evaluated pose and qualified
provider geometry to project the Original center without decoding or proxy
access. Existing outside-range path refusals and non-creep Follow refusals
remain. Pending picture
work keeps the full macro capture; Escape or `:record-cancel` cancels it before
it can submit delayed framing. See [semantic macros](SEMANTIC_MACROS.md#ranged-zoom-and-creep)
and [ranged framing qualification](qualification/ranged-framing-macros-2026-10-18.md)
for implementation and verification scope. The
[scoped range qualification record](qualification/scoped-framing-ranges-2026-10-25.md)
tracks the added occurrence-range boundary.

The save message describes the committed framing (`Framing saved: 1.350×
following Target 1`), and the inspector shows `Path · 1.00–1.35×` for camera
paths. The `zoom` replay checks the center fallback and its message, the
target follow and its displayed pose, `:zoom … target=center`, `:creep … target=current`,
refusal to flatten a path, `:zoom off`, a ranged punch-in at exact frames,
refusal of `,c` on a path, black punctuation, a recorded `:zoom` replayed after
Undo, and the two-target refusal of `,z` followed by a quoted-label `:zoom`.

### Face proposals

Camera's `f` picker requests proposals for its exact stopped Original picture
through the shared picture-proposal service. One qualified source preparation,
indexed decode, BGRA buffer and Vision image handler serve both face rectangles
and objectness saliency. Saved targets, center and corners keep their positions;
detected faces append, followed by salient regions. Each kind is ordered left
to right, then top to bottom, with independent numbering. Each page has digits 1–9. Tab advances a page, Shift+Tab
returns, and the inspector has matching page buttons. Pending, empty and failed
detections leave ordinary choices available. Closing and reopening the picker
refreshes proposals; a completion for a closed or changed Camera cannot act.

Selecting a numbered face or region explicitly saves only that proposal as an ordinary
target, then continues Camera centered on it. The scale and complete existing
camera path remain temporary, including a Follow's clock and scale animation.
Enter applies framing as a separate undoable edit. Escape restores entry
framing while keeping the explicitly accepted target. A refused target save
keeps the previous draft and permits another selection. Detection alone saves
nothing. These target and framing history steps differ from the combined
command below.

`target=face:N` (specification §6.4 `:zoom 1.35 target=face:2 curve=step`)
names the `N`th face Apple Vision's face rectangle detector proposes in the
stopped picture, counted from 1 left to right (then top to bottom). Detection
proposes; it never edits. The command is captured and its picture displayed
exactly as for any `:zoom`/`:creep`; the app then sends one `DetectPictureProposals`
command for that indexed Original picture and keeps editing while the project
service's Faces-only job runs (`deadpan-track picture-proposals`, see
[tracking](TRACKING.md#face-detection-mode)). The footer says
`Finding Face N in the displayed picture… Nothing changes unless that proposal is found.`

When the detection finishes, the command applies only if its session,
revision, Visual/Edit range, selected beat, Repeat scope, cursor and Sequence
scope are exactly those captured when it started, and the job reports the same
session, revision, asset and picture. Then face `N` becomes an ordinary
untracked target (`face-K` id, `Face N` label; the label adds the id if
`Face N` is taken) over the same span Camera's `n` gives a drawn rectangle:
from the analysed picture to the next stored shot boundary or the end of the
measured video. Its region is the face rectangle in millionths; it has no
samples, corrections or provenance (core `TargetRule` is closed to tracking).
The framing is built exactly as `target=ID` would build it for that target, and
the target and the framing are committed as **one** `Compound` transaction
(`SetTarget` then `SetFraming`) expecting the entry revision: one history entry,
one Undo removes both. The message identifies the framing and new detected
target, and explains that one Undo removes both.

Refusals make no edit: no face in the picture (`No faces were found in this
picture.`), `face:N` beyond the count (`face:3 is out of range: this picture
has 2 faces (face:1–face:2), numbered left to right.`), any context change
while detecting, a picture that is not an Original moment, detection
unavailable or failed (worker missing, unsupported platform, worker/protocol
error), `face:0`/`face:65` (command line), recording a macro, and a single
Repeat play (scoped Camera targets; frame the face from an ordinary beat
first). A whole-beat step follows the face target live; pictures of the beat
before the analysed picture lie outside the target's span and use the
follow's fallback center. Eased and ranged forms sample the target where they
land, so a range starting before the analysed picture refuses as "not visible".

The `faces` replay drives the real command line and project service: the real
installed worker finds no face in the replay fixture and refuses; then (with
only the worker's reported faces scripted) a delayed detection keeps the
editor responsive, `face:2` saves `Face 2` and follows it at 1.35×, one Undo
removes both, a cursor move during detection refuses the stale completion,
`face:3` of two refuses with the count and `face:0` is refused at entry.

`target=region:N` uses the same captured-context and atomic `SaveFramed` path,
requesting only salient regions. It creates `region-K` / `Region N`, preserves
the measured source display aspect, and refuses absent regions, out-of-range
numbers, stale context, scoped direct commands and macro recording. Camera's
picker can accept a proposal through its existing scoped target-save route.
`detect-regions PROJECT --at PTS [--asset ID] [--include-faces]` lists results
headlessly without edits; the flag requests both kinds atomically.

Objectness saliency uses Apple Vision revision 2 and supplies rectangles without
confidence scores, semantic names or a relevance ranking. The face request keeps
its existing default revision policy. Each reports its actual runtime separately.
At most 64 proposals per kind and one saliency image observation are accepted;
these are safety limits. A successful empty collection differs from an absent
unrequested collection. Any selected detector failure rejects the whole batch.
Vision rectangles may extend beyond the image: finite positive rectangles are
clipped to the image before upright rotation, and wholly outside boxes disappear.
Invalid numeric geometry fails. See the [qualification record](qualification/camera-regions-2026-10-25.md)
for actual checks and remaining verification.

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

Saved point and region targets, keyboard creation, following and selected-region
tracking are implemented (above, [targets](TARGETS.md), [tracking](TRACKING.md)).
General region proposals and pointer target drawing are also implemented.
Whole Source, Hold, Sequence, Repeat and Retime following creep animates
scale while retaining the live target in ordinary selection and explicit Play
scopes, as described above. Follow centers are placed on a letterboxed or
pillarboxed canvas for targets that record their display aspect
([qualification](qualification/letterbox-follow-2026-10-15.md)). Centered per-play scale
escalation is in [Repeat escalation](REPEAT_ESCALATION.md). Ungroup moves a group's camera path onto its unframed
children, each windowed to its own interval of the same path (`FramingClock::RetainedOutput`), so every picture keeps
its pose (`ungrouping_a_framed_group_moves_its_camera_path_onto_its_children_unchanged`); a child with its own framing
still refuses. Target-centered escalation, nested native selection, arbitrary temporal envelopes, captions, cutaways, and the remaining
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
