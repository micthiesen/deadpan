# Whole-Hold following creep

Base: efd84b88324d1b71dfe339b19c097c376e64783f. Full normative goal stays active.
Point targets are finished in db729c592b5a83d684d35ca0978b12b67a94a75c;
its immutable evidence must never change. Foreign fuzz/Cargo.lock and .claude/
remain outside this task.

Add whole ordinary Hold authoring to the existing Source-only implicit Keep
Follow creep. Retain existing FollowScaleTransition payload, exact factor,
independent live/fallback bases and OwnerOutput semantics. No version bump,
media mutation or new timing representation.

Replace framing_zoom::Shot.ordinary_source with
ordinary_owner: Option<framing_zoom::OrdinaryFollowOwner>, a small Copy enum
with Source and Hold variants. None means this invocation has no whole
ordinary Source/Hold authority. It does not limit raw Follow payload validity.
Native scope must still be unscoped, with no enclosing Repeat, direct child
of the current ordinary Sequence, no Visual range and an existing plain Follow
without a transition. Core semantic ordinary replay classifies the actual
selected node; scoped replay always passes None. Keep all other owner/range/
existing-path refusals. Explicit target replacement retains current behavior.

A Hold's intrinsic output duration is its owner extent. Verify live/fallback
scales across that complete duration, provider clock and captured geometry.
Never change its duration, provider, audio recipe or framing input geometry.
Where target support is absent, use the existing explicit fallback semantics.

Worker owns non-App Core/CLI implementation and relevant tests only. Root owns
all deadpan-app source, native harness, docs, review integration, qualification,
Git and progress artifact. Do not overwrite concurrent work. No agent worktrees.
Coordinate Cargo in /tmp/deadpan-follow-creep-2026-10-24/target with the pinned
FFmpeg prefix. All earlier Cargo/runtime jobs are finished.

Captured geometry prerequisite (confirmed after initial design):
Normal native :hold ALWAYS retains CapturedFraming, even identity/unframed.
Current Plan/Camera skipped Follow for any captured context, so merely enabling
Hold authoring would be fallback-only. Include real exact target projection.
Worker captured_target_projection owns all Plan production/tests and adds
follow_pose_in_context(target,asset,point,canvas,context,inner,scale); existing
follow_pose remains None-context wrapper. Root has changed Camera layer_poses
and target_center_at to call the new API. Do not qualify only an uncaptured
synthetic Hold as the native feature.

Native witness design:
Fresh 120-frame counter project. At frame10 create a point target1=(.58,.5)
in Camera, save target and leave Camera without framing. Set Source zoom2
target=target-1, then go to Edit40 and :hold 12f. Freeze uses Original39 and
captures Source crop scale2/center.58. Select the actual ordinary root Hold
with production beat navigation if insertion did not already select it.

On that Hold, Camera n,n opens point centered at captured view center
(source .58,.5);10l produces point2=(.68,.5). Save separately, t directly,
verify actual resolved Hold center is (.7,.5) after the captured crop,
distinct from fallback(.5,.5), Enter to commit. :creep from=1.1 to=2.2
curve=linear must keep target2 and OwnerOutput duration12; exact factor2
scales live/fallback bases independently. All Hold local frames0..11 show
center.7, sourceFreeze39 and frame-center scale interpolation. Recipe picture
context/video/audio, all other nodes, both cursors, total132 remain unchanged
by creep. Undo/Redo, record/replay/dot and Camera save/cancel preserve intent.

Captured clipping negative: correct point2 at its first source moment from
.68 to.98 (30l). The retained Source crop clips it before any outer framing;
Follow must fall back to(.5,.5) while scale animation continues, never
resurrect the cropped target. Undo restores point.68 and live center.7.
Final export fixture should end in restored resolved Follow, full132pictures
and canonical full PCM window (origin based B(132), do not assume192192).

Plan exact stage order: source-aspect Fit/Fill -> first captured pose -> clip;
later poses each transform then clip; fit whole previous canvas into next
captured canvas, then repeat; final Fit captured canvas to current canvas,
then live inner posed layers. A Fill transform can place a raw point outside
before the first pose; don't clip prematurely before that pose.
