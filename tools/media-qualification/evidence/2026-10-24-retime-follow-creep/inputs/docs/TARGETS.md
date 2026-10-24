# Attention targets

Specification §4 (Attention · Target), §5.4 and §11.3. A target is a point or region of
the Original followed over source time. It is evaluated in source time, so it
follows its subject through any retiming, repetition or freeze of the pictures
that show it. Targets name no person.

## Model (`deadpan_core::AttentionTarget`)

- `asset` and half-open source `span` inside that asset's video.
- `region`: the initially selected `TargetGeometry`, either a rectangle by
  center and size or a point by center alone, in millionths
  of the displayed, uncropped picture (`(0,0)` top-left, `(1,1)` bottom-right),
  the same normalized source coordinates framing centers use. Rectangle JSON
  remains `{"center":[x,y],"size":[w,h]}`; point JSON is `{"center":[x,y]}`.
  Points have no implied area. A target's initial geometry, samples and
  corrections must all have the same kind.
- `samples`: tracked pictures, strictly ordered source ticks inside the span,
  each with region, confidence (thousandths) and `TrackState`
  (`Tracked`, `Interpolated`, `Lost`). A lost sample holds the last confident
  position until a manual correction.
- `corrections`: manual regions from a source time onward. The latest
  correction or sample at or before a time applies, and a correction wins at the
  same time. `correction_range` is the range a correction invalidates: from it to
  the next correction or the end of the span.
- `provenance` (optional): how the samples were produced. `rule` is a closed
  `TargetRule` (`deadpan-track-1`), `stop` a closed `TargetStop` (`range_end`,
  `shot_boundary` or `picture_limit`: why the first run ended where the span
  ends), and `engine` one printable line of 1–96 bytes (no control characters
  or Unicode line/paragraph separators) naming the tracker of the latest run.
  It is a record, never an authorization; a hand-made target omits it, and
  documents without it are unchanged.
- `display_aspect` (optional `ExactRatio`): the exact upright aspect of the
  picture the regions are normalized to, visible (clean aperture) width times
  the sample aspect ratio over visible height, inverted for a quarter or
  three-quarter turn (`deadpan_core::display_aspect`, and
  `deadpan_render::FrameMetadata::display_aspect` for a decoded picture).
  It must be positive, at most 64:1 either way, with reduced terms of at most
  2^48. Every creation site records it: Camera's drawn rectangle and
  `target=face:N` from the exact Original picture, tracking (`track --save`,
  the app's `T`) and face proposals from the Original's qualified stream
  geometry. Corrections and re-tracking keep it. Absent targets serialize as
  before.

Between two consecutive samples in the same moving state (both `Tracked` or both
`Interpolated`) with no correction after the first up to the second,
`region_at` interpolates linearly in exact source time, rounding each center and
size component half to even to a millionth (sizes at least one); points
interpolate only their centers. Nothing
interpolates across a `Lost` sample, a change of state, a correction or from the
initial region; there the earlier entry holds. Sparse or strided paths therefore
move smoothly instead of stepping. Where the exact interpolation overflows (an
extreme fractional source point), `region_at` returns `None`, as outside the
span, so a follow uses its fallback rather than a region the target does not
describe.

Bounds: 64 targets, 4,096 samples and 256 corrections per target, and 32,768
samples per project. Hosts compact denser tracker output before saving; the
tracking host drops only samples this interpolation reproduces within a stated
tolerance ([tracking](TRACKING.md#saving-as-an-attention-target)).

`SetTarget` and `DeleteTarget` are ordinary reversible commands. They add no
picture time. The document map is `targets`; patches carry `targets` changes.

## Following a target

`FramingValue::Follow { target, scale, fallback }` centers a framing layer on
the target at the source time of the picture shown, at its evaluated scale.
A whole ordinary Source, Hold, Sequence, Repeat or Retime can retain a live-target scale transition
([following creep](FRAMING.md#zoom-and-creep-commands)). The plan
resolves it after descending to the picture (`RenderPlan::resolve_follows`): the
target point is carried outward through every inner posed layer, innermost
first (`p' = (p - center) * scale + 1/2`), so an outer follow sees the subject
where inner framing put it. On a Sequence the target uses each child's current
Original time across cuts, while scale uses the complete group's output clock.
On a Repeat selected from outside, the same owner clock spans all plays and
intervening gaps while the provider clock resets or holds. Per-play escalation
remains an inner scope and applies to the following gap.
On a Retime the scale uses its output clock, while the target uses the exact
mapped Original point. Fractional coordinates remain exact even when several
output pictures decode the same source ordinal.
Before that, a target with a `display_aspect` has
its center placed on the presentation canvas exactly where the renderer fits
the whole picture (`deadpan_core::fit_source_point`, the `Fit` letterbox or
pillarbox): along the axis the picture fills the coordinate is unchanged, along
the other it is scaled about the middle by the covered fraction, in exact
rational arithmetic. A 16:9 Original on a 1080x1920 Shorts canvas covers
81/256 of the height, so a subject at y = 0.4 is centered at
1/2 - (1/10)(81/256) = 1199/2560 of the canvas. Preview, Camera and export use
the same `follow_pose_in_context` with the canvas from the presentation basis.
Resolved poses are quantized to the framing grid.
The fallback applies where the picture is not the target's asset, lies outside
its span, is clipped by a live inner scope, or the centered pose is out of range
(including arithmetic overflow);
evaluated on its own, a follow is its fallback. An endpoint-held Source picture
uses its selection's last moment. Under a pause, the target center traverses
each retained canvas's Fit/Fill, ordered poses and clips, then the current
canvas and live inner framing. A later zoom-out cannot restore a point clipped
by an earlier captured or live scope. This also applies without captured
geometry; the earlier context-free exterior-center behavior is corrected.
Captured projection requires the target's
measured display aspect; absent aspect, lost coverage or arithmetic overflow
uses the fallback. The retained crop, provider and Original clock do not change.
Copies of a followed beat
carry the target and its asset; a paste adds the target only where the
destination lacks it, so the destination's current target wins.

A document refuses framing that follows a missing target, so `DeleteTarget`
refuses while framing still follows it. Camera on a follow layer changes its
scale and the fallback; the target supplies the center.

## In the native app

Camera picks, draws, follows and corrects targets ([framing](FRAMING.md#native-interaction)):

- `f` lists saved targets covering the stopped picture, center and corners,
  then detected face proposals. Each page has digits 1–9; Tab and Shift+Tab
  reach the other pages. The viewer draws saved targets at `region_at` of the
  picture's exact source time with its state (`drawn`, `corrected`, `tracked`,
  `interpolated`, `lost · holding`).
  With several saved targets, picker overlays use compact digits and the
  inspector retains full names and states. Outside the picker, the active
  target keeps its full label and secondary targets keep their markers.
  While drawing or correcting, only the geometry under edit is labelled.
- `n` opens a new target with the keyboard; `n` in that editor switches
  rectangle / point. Rectangles have center, width and height; points have
  only center, and Tab stays there. Both use
  arrows or `h/j/k/l`, Shift for 5%, counts, and Enter saves one undoable
  `SetTarget` with a fresh `target-N` id and `Target N` label, no samples and no
  provenance. Its span starts at the displayed picture's indexed PTS, where the
  target was selected, and ends at the next stored shot boundary of the
  Original, or at the end of the measured video without a shot analysis. This
  matches tracking, which starts from the selected picture and stops at the
  next cut; pictures earlier in the same shot are outside the target and a
  follow keeps its fallback there.
- `,z` and `:zoom S target=…` follow a target from outside Camera, and
  `:creep … target=…` eases toward its position at the displayed picture
  ([zoom and creep](FRAMING.md#zoom-and-creep-commands)).
- `:zoom S target=face:N` and `:creep … target=face:N` use face `N` that Apple
  Vision proposes in the displayed picture, left to right. Detection runs in
  the background and only proposes; the explicit command saves the face as an
  ordinary untracked target (`face-K`, `Face N`, span as for `n`) and frames
  the beat toward it in one undoable `Compound`, refusing when no face, too few
  faces or a changed context ([face proposals](FRAMING.md#face-proposals)).
  Such a target can then be tracked with `T` or `:track face-K` like any drawn
  one.
- `:target-rename TARGET New label` relabels a target named by id or label as
  one undoable `SetTarget`; labels stay unique (case-insensitive), so a target
  can always be named by its label. `:target-delete TARGET` deletes it as one
  undoable `DeleteTarget`, or refuses and names every beat whose framing or
  escalating zoom still follows it (`a_target_in_use_refuses_deletion…`).
- `t` follows the picked target; `c` corrects it at this picture; `T` tracks
  rectangles. Corrections keep the saved geometry kind. A point correction
  replaces the initial point at the span start or adds a manual correction
  at a later picture, invalidating samples only until the next correction
  or span end. It never launches tracking. Tracking a point refuses before
  work or history changes, because a point has no tracking rectangle.
  An untracked rectangle can be corrected at its span start; later corrections
  require tracking first. A correction of a tracked rectangle re-tracks only
  the correction's range
  ([tracking](TRACKING.md#in-the-app)).

Saves are ordinary revisions with no node and no picture time: the cursor,
selection and Repeat scope stay where they are, Camera continues on the saved
revision, and Undo removes or restores the target. The beat inspector names a
followed target by its label (`Follows Target 1 · 1.10×`) and lists every saved
target with its state at the current picture and whole-span summary.

## Limits

Target regions are in upright source picture coordinates. Follows of targets
that record their `display_aspect` are placed on canvases of another aspect
([qualification](qualification/letterbox-follow-2026-10-15.md)). A target
saved without it (before 2026-10-15) keeps source coordinates on such a canvas
and centers the wrong point; re-saving it (draw, track or `track --save
--replace`) records the aspect. Targets are
created and corrected headlessly (`track --save`, `track-correct`; see
[tracking](TRACKING.md)) and in Camera, and saved from a face proposal by
`target=face:N`. `detect-faces` lists proposals headlessly without editing.
Camera's numbered face proposals use the same service. Selecting one saves
only that face as a target, keeps Camera open and centers its draft; Enter
commits framing separately. Escape retains the explicitly accepted target
while restoring entry framing. Detection alone never edits.

Points and rectangles share the saved picker, Follow, creep, rename, delete,
copy and history paths. The viewer draws points with a cross and center
marker. Region-based AI conditioning reports `point_target` unavailable for
points, retaining their identity without inventing a subject rectangle.

Pointer rectangle dragging, general
(non-face) region proposals and quality evidence on real people remain open.
