# Next slice: ordinary Retime Follow creep

## Recommendation

Add Keep-center Follow creep to a whole directly selected Retime whose owner is outside any enclosing Repeat occurrence. Retain current refusals for Visual ranges, selected/explicit occurrences, and a Retime under an enclosing Repeat. Do not limit the Retime’s child structure. Retime is the remaining simple owner after Source/Hold/Sequence/Repeat, and its non-unit owner clock provides a useful exact test of the separation between scale time and provider time.

## Minimal entrypoints

- crates/deadpan-core/src/framing_zoom.rs:113-125: add OrdinaryFollowOwner::Retime and update the Shot comment. The core builder at :236-264 already constructs OwnerOutput transition once owner is Some and range is absent. No numeric/wire/model addition is indicated.
- crates/deadpan-app/src/preview/camera.rs:853-880: ordinary_follow_owner rejects session.scoped and any enclosing session.scope.repeats, then requires selected node be direct child of current Sequence. Add Retime in the direct selected-node kind match only. Do not relax the guards or treat a Retime ancestor as selected-owner authority.
- crates/deadpan-core/src/semantic/planner/framing.rs:160-188: shared semantic Zoom planner maps selected node kind to OrdinaryFollowOwner. Add Retime here. Scoped authorization remains separate in validate_scoped_context at :6-65; keep it rejecting scoped edits before this whole-owner behavior is considered.
- Keep Zoom Keep semantic recording/replay/dot path shared with current Source/Hold/Sequence/Repeat. The range stays the existing local selected-owner range and must still refuse even if it covers the full Retime.

## Existing timing and Follow evidence

- crates/deadpan-plan/src/plan.rs:889-914 compiles Retime from DerivedRetimeClock; picture traversal at :1638-1648 maps Retime-local position through start + local * scale before entering the child. The Retime framing itself was already pushed at :1469-1494 using its own local position and effective duration. This is the required owner/provider separation: transition scale evolves over Retime output; Follow resolves from the resulting Source picture point.
- crates/deadpan-plan/src/picture.rs:64-78 exposes Picture::follow_point as the exact SourcePoint or Freeze source time. It is already after the plan’s Retime transform.
- Existing positive neighboring test: crates/deadpan-cli/src/framing/tests.rs:325-389, semantic_framing_uses_live_follow_pose_at_retimed_source_clock_instead_of_fallback, checks a child Source Follow through a non-unit Retime and compares semantic preview pose with RenderPlan. It does NOT test Follow authored on the Retime owner or an owner scale transition.
- Existing negative/authority witnesses: crates/deadpan-app/src/preview/harness/sequence_follow_creep.rs:492 onward refuses Keep creep on a selected Retime; crates/deadpan-cli/tests/project_commands/macros/framing/sequence_follow_creep.rs:490-525 and repeat_follow_creep.rs:608-640 exercise raw valid Follow on Retime plus authoring refusal. Convert the ordinary whole-owner negative into a positive; retain the scoped-occurrence refusal separately.
- Raw scale behavior is already generic in crates/deadpan-core/src/framing.rs:427-461 and FollowScaleTransition curve/integer-oracle tests :709 and :769. Retime does not require another interpolation implementation.

## One concise decisive fixture

Adapt sequence_follow_creep native scenario: its two Source children and target crossing the seam already provide distinct provider times. Wrap the selected Sequence in a non-unit Retime (for example 2x speed), select the Retime owner, author a fixed Follow there with the existing Zoom target command, then invoke Keep creep. At output frames before/at/after the mapped child seam, assert independent exact source points equal start + local*scale, centers follow the moving target at those points, and the Follow scale matches the owner-output integer oracle over Retime duration. Compare complete audio-plan spans before/after the SetFraming operation; verify one Undo/Redo, Camera preserves transition, range and enclosing Repeat refusals, and semantic record/replay plus dot retain Keep intent. Existing source fixture already has a native Kestrel-checked workflow and audio witness, so this is a narrow owner substitution rather than a new UI flow.

## Proof limits

Pure/plan coverage should include non-unit scale and nonzero mapping start to avoid only testing a zero-origin 2x case. Native replay can prove actual Retime selection, Camera and durable history; emitted-media audit must verify every emitted picture and unchanged PCM/gain at zero sample offset. These do not prove subjective tracking quality or acoustic listening, and a single 2x fixture does not qualify all Retime speeds, nested Retime, or Repeat-contained Retime. The latter remains deliberately refused as an authoring scope.

No source edits, Cargo commands, native runtime, or Git operations were performed.
