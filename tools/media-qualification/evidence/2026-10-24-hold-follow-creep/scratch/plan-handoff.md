# Captured target projection: Plan handoff

Five owned Plan paths are frozen in plan-freeze.json (SHA256
1764b416ec5b9fed6b22d9bb7445f3d306cbe6723f44ea42383b8f8ed02b41f9).
No Core, CLI, render, App, dependencies, schema or unsafe changes in this lane.
No active Cargo/runtime processes; shared target released to Root.
A read-only git diff --check of Plan passed; no Git mutations.

API: deadpan_plan::follow_pose_in_context(target,asset,point,canvas,
context:Option<&CapturedFraming>,inner,scale). Existing follow_pose wraps None,
preserving its existing source-aspect/legacy/no-context behavior and exterior
live-center semantics. lib.rs exports both. Plan resolve_follows receives
picture_context.as_deref(), so Hold/Repeat-gap retained geometry can resolve
ordinary and escalating Follow layers instead of unconditionally falling back.

Projection is allocation-free exact ratio arithmetic in follow_context.rs.
It validates bounded retained context, requires measured target display_aspect,
and maps the target's center (point or rectangle) into the first captured canvas
under its Fit/Fill. Each captured layer transforms before its own inclusive
point clip; an empty layer list is one identity clip. A Fill fit does NOT clip
before the first pose, because that first scope may bring the point in. Every
later stage fits the complete previously clipped canvas, including its bars,
then repeats its layers/clips. The last captured canvas Fits into the live
canvas. Context-bearing live inner layers also clip after every scope, so a
later zoom-out cannot revive a target removed by a preceding clip. Missing
aspect, invalid context, source/span mismatch, clipping and checked arithmetic
overflow return None, preserving the existing evaluated Follow fallback.
There is no floating-point spatial math or clock conversion.

Tests:
- plan-focused-2:5 passed, including deterministic256-case identity-capture
  equivalence property for both point and rectangle geometry and arbitrary
  current canvas aspects.
- plan-picture-all:127 passed, including all existing picture plan cases.
  Exact commands/environment/source-before/after/durations are in those JSON
  receipts; both sources_unchanged true.
- First plan-focused-1:4 passed/1 failed, exit101. The Hold fixture assigned
  Follow before the helper admitted its target map, so the helper correctly
  rejected InvalidTree before projection ran. Corrected by assigning both
  framings and target map in one JSON before document validation. Raw streams
  retained; first failed fixture source was not separately snapshotted.
  That first run also retained a Proptest SourceParallel warning. The final
  property uses an explicit fixed seed and no file persistence, so its256
  inputs reproduce without creating repository scratch files.

Independent static fraction witnesses cover landscape/portrait Fit and Fill,
first-pose-before-clip, retained identity clip, nested differently-shaped
canvases, final Fit, irreversible captured and live child clipping, missing
aspect, zero live canvas, empty context, source/span mismatch and exact
denominator overflow. The real Plan Hold fixture uses nested captured geometry,
a frozen Original SourcePoint, a12-frame FollowScaleTransition and an outer
Follow: all12 samples retain the same frozen point/capture, resolve the Hold
center, vary scale on the complete output clock, and center the outer Follow
through the inner operation. Recipe/provider/audio/duration remain identical.

No native renderer/runtime/media test or performance claim in this lane.
Root owns Camera integration, full cross-crate checks/reviews/native/evidence.
