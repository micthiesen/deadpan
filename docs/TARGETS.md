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

Bounds: 64 targets, 4,096 samples and 256 corrections per target, and 32,768
samples per project. Hosts compact denser tracker output before saving.

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
its span, or the centered pose is out of range; evaluated on its own, a follow
is its fallback.

A document refuses framing that follows a missing target, so `DeleteTarget`
refuses while framing still follows it. Camera on a follow layer changes its
scale and the fallback; the target supplies the center.

## Limits

Target regions are in source picture coordinates. When the canvas aspect
differs from the source's, the innermost layer's input is the fitted canvas,
and follow centers are not yet corrected for that letterbox. Tracking itself,
keyboard target creation and the Camera picker integration are tracked in
[tracking](TRACKING.md) and [framing](FRAMING.md).
