# Next target/framing slice after whole Source/Hold Follow creep

## Recommendation

Extend Keep-center Follow creep to a whole ordinary Sequence group (a selected Sequence with no Repeat/Retime/other occurrence scope), over that group’s complete OwnerOutput duration. Keep Repeat/Retime/selected-play/Visual-range authoring explicitly refused.

This directly advances §8.2’s “Slow creep” across a multi-beat structural owner. It has clearer product value than another detector proposal: existing target paths already span source time, and the shared plan already evaluates a Follow under owner clocks. It exercises the meaningful difference between a single Source and a changing provider across a cut, while avoiding the nested occurrence clocks that make Repeat/Retime authoring broader. General non-face proposals remain an important DP-08 gap, but are a separate detection/selection surface rather than a prerequisite to using existing authored targets.

## Exact current admission gaps

- Core builder: crates/deadpan-core/src/framing_zoom.rs:145 exposes FOLLOW_CREEP_REFUSAL; build at :149 onward permits Keep creep on an existing Follow only if shot.ordinary_owner.is_some() and shot.range.is_none() (:236-244).
- Current owner enum and Shot fields are near :90-144. Existing admitted variants are Source/Hold; refusal currently says “Source or Hold”.
- Native owner bit: crates/deadpan-app/src/preview/camera.rs:853-879, ordinary_follow_owner. It refuses scoped/repeat paths and requires the node be a direct child of current parent Sequence (:859-868), then admits only Source/Hold (:869-879). Camera passes it to zoom::Shot at :1400-1408.
- Macro semantic resolver also builds this shot/owner context; search SemanticZoomCenter/ordinary owner in crates/deadpan-core/src/semantic/planner/framing.rs (semantic resolution/planning) and its CLI integration/replay callers. Share policy so typed CLI, replay/dot and Camera cannot diverge.
- docs/REQUIREMENTS.md:4061 now needs wording aligned with whole ordinary Source/Hold admission; its remaining gap is beyond those owners. docs/FRAMING.md:219,237-253 documents the Source/Hold boundary and current refusal for single-play/other owners.

## Smallest honest boundary

Define ordinary Sequence as the directly selected Sequence owner with no enclosing Repeat/Retime occurrence scope and no active Visual range. Do not reinterpret the selected child beat or apply across nested repeats. Preserve PATH_REFUSAL for animated Follow / Envelope unless explicit target replacement is requested. Maintain invocation-relative semantic Keep intent so macro/dot resolves the current live target at application time.

Risks: target time must resolve from each provider picture through ordinary sequence children, including different Source intervals on either side of a cut; scale uses Sequence output clock, not provider/source local time. A missing, clipped, or Lost target must retain existing fallback/last-region rules; measured targets project through retained Hold Fit/Fill/pose/clip geometry before live framing, with clipped targets falling back; audio, duration, children, and independent framing remain unchanged. One SetFraming history action, correct Undo/Redo, no flattening of child paths. Ensure selection/context identifies a Sequence rather than routing the command to its child or root.

## Decisive test shape

1. Core/plan: Sequence with two Source children whose same saved target has distinct moving paths and a boundary between them. Install fixed Follow on the Sequence, apply Keep creep. Independently check first/last scale, interior Q32 values, fallback basis, target centers on both sides of the cut, Lost continuation, and that Follow pose uses current child-provider time while transition uses Sequence OwnerOutput. Check child framing and sound/audio-plan mapping unchanged.
2. Native replay: group two Sources, set a region target with distinct moving samples crossing the seam (or reuse a target spanning both providers), frame the selected whole Sequence with Follow, invoke :creep with Keep, then verify rendered stopped frames around both sides of seam equal the shared plan; one Undo/Redo; Escape/no-change Camera Enter and explicit t release; recording, named replay and dot preserve semantic Keep intent. Add refusal for Visual range and nested Repeat occurrence.
3. Media: reuse the 120-picture CFR counter/audio Original and existing independent pose/PCM helpers. Make the Sequence boundary visible at a known frame, author moving target regions across both Sources, ensure every actual output frame is checked (especially seam-adjacent pictures), compare all canonical PCM and gain bits before/after at zero offset, then portable-copy/reopen and verify history. New structural owner needs its own full media check; do not extrapolate the existing Source/Hold exports. Current Hold qualification reports 132 pictures and 211,411 full audio samples at zero offset; native Hold replay reports 67 checks.

Likely extension points: shared pose oracle/fixture builders in crates/deadpan-cli/tests/preview_export/follow_creep.rs; export assertions in follow_creep_export.rs; native macro replay in crates/deadpan-app/src/preview/harness/zoom/follow_creep.rs and Hold counterpart preview/harness/hold_follow_creep.rs. Transition numerics, captured geometry, and generic serialized transition need no redesign.

## Why not the nearest alternatives

- Repeat-wide creep is the strongest follow-up after Sequence because §8 names a repeated group, but it introduces a multi-play owner clock plus interaction with per-play escalation and each occurrence; larger follow-up.
- Retime or selected-play creep adds provider/output affine maps and isolation semantics, also larger.
- Whole-Hold is now admitted and its captured-geometry path is implemented: measured targets project through retained Fit/Fill, captured poses and clips, then current live framing. The final Hold qualification reports 67 native checks and media verification of 132 pictures plus 211,411 full audio samples at zero offset. Treat this as current passing coverage, not a remaining blocker.
- General non-face proposals remain explicitly missing and should be a separate DP-08 slice. Saved rectangle authoring and face proposals already supply usable targets; another proposal source would not exercise the new Follow transition.
