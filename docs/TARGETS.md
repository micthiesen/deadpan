# Attention targets

Specification §4 (Attention · Target), §5.4 and §11.3. A target is a region of
the Original followed over source time. It is evaluated in source time, so it
follows its subject through any retiming, repetition or freeze of the pictures
that show it. Targets name no person.

## Model (`deadpan_core::AttentionTarget`)

- `asset` and half-open source `span` inside that asset's video.
- `region`: the initially selected rectangle by center and size, in millionths
  of the displayed, uncropped picture (`(0,0)` top-left, `(1,1)` bottom-right),
  the same normalized source coordinates framing centers use.
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

Between two consecutive samples in the same moving state (both `Tracked` or both
`Interpolated`) with no correction after the first up to the second,
`region_at` interpolates linearly in exact source time, rounding each center and
size component half to even to a millionth (sizes at least one). Nothing
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
the target at the source time of the picture shown, at a fixed scale. The plan
resolves it after descending to the picture (`RenderPlan::resolve_follows`): the
target point is carried outward through every inner posed layer, innermost
first (`p' = (p - center) * scale + 1/2`), so an outer follow sees the subject
where inner framing put it. Resolved poses are quantized to the framing grid.
The fallback applies where the picture is not the target's asset, lies outside
its span, or the centered pose is out of range (including arithmetic overflow);
evaluated on its own, a follow is its fallback. An endpoint-held Source picture
uses its selection's last moment. Under a pause with captured geometry, follows
keep their fallback until that geometry is mapped too. Copies of a followed beat
carry the target and its asset; a paste adds the target only where the
destination lacks it, so the destination's current target wins.

A document refuses framing that follows a missing target, so `DeleteTarget`
refuses while framing still follows it. Camera on a follow layer changes its
scale and the fallback; the target supplies the center.

## In the native app

Camera picks, draws, follows and corrects targets ([framing](FRAMING.md#native-interaction)):

- `f` lists saved targets covering the stopped picture before the center and
  corners; digits pick one. The viewer draws each at `region_at` of the
  picture's exact source time with its state (`drawn`, `corrected`, `tracked`,
  `interpolated`, `lost · holding`).
- `n` draws a new rectangle with the keyboard (Tab: center, width, height;
  arrows or `h/j/k/l`, Shift for 5%, counts) and Enter saves it as one undoable
  `SetTarget` with a fresh `target-N` id and `Target N` label, no samples and no
  provenance. Its span starts at the displayed picture's indexed PTS, where the
  rectangle was selected, and ends at the next stored shot boundary of the
  Original, or at the end of the measured video without a shot analysis. This
  matches tracking, which starts from the selected picture and stops at the
  next cut; pictures earlier in the same shot are outside the target and a
  follow keeps its fallback there.
- `t` follows the picked target; `c` corrects it at this picture; `T` tracks it.
  A correction of an untracked target replaces the initial rectangle at the
  span start or adds a manual correction at that picture. A correction of a
  tracked target re-tracks only the correction's range
  ([tracking](TRACKING.md#in-the-app)).

Saves are ordinary revisions with no node and no picture time: the cursor,
selection and Repeat scope stay where they are, Camera continues on the saved
revision, and Undo removes or restores the target. The beat inspector names a
followed target by its label (`Follows Target 1 · 1.10×`) and lists every saved
target with its state at the current picture and whole-span summary.

## Limits

Target regions are in source picture coordinates. When the canvas aspect
differs from the source's, the innermost layer's input is the fitted canvas,
and follow centers are not yet corrected for that letterbox. Targets are
created and corrected headlessly (`track --save`, `track-correct`; see
[tracking](TRACKING.md)) and in Camera. Point targets, renaming and deleting
targets in the app, pointer rectangle dragging and face/region proposals remain
open.
