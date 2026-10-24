# Next slice: whole Repeat Follow creep

## Recommendation

Add Keep-center Follow creep for a directly selected whole Repeat owner when it is outside any enclosing Repeat occurrence. Keep selected-play, Visual-range, and Retime-owner creation refused. Do not constrain the Repeat’s descendant structure: nested Source, Sequence, Hold, Retime, explicit gap Hold, and default gap remain content under the selected owner. This is the next normative step after Source, Hold, and ordinary Sequence: §8.2 explicitly calls for creep over an entire repeated group.

Prefer this over ordinary Retime creep. Repeat’s existing plan already evaluates its owner framing on Repeat-local output coordinates and duration; ordinary Retime creep would add a less explicit product behavior and make affine provider/output mapping the centerpiece. Repeat adds real interaction with play boundaries and escalation, which is the valuable bounded test.

## Admission points

- Core schema and builder: crates/deadpan-core/src/framing_zoom.rs:113-126 defines OrdinaryFollowOwner and Shot. Add a Repeat owner variant; build’s Keep-Follow Creep admission at :236-264 requires an admitted owner and no range. Keep raw Framing FollowScaleTransition validity generic; existing raw tests already exercise the numeric transition independent of authoring eligibility.
- Native Camera: crates/deadpan-app/src/preview/camera.rs:853-880 ordinary_follow_owner first rejects session.scoped and any session.scope.repeats, then requires the selected node be a direct child of the current Sequence, and classifies Source/Hold/Sequence. Add Repeat only in that direct selected-node match. Retain both occurrence guards: do not authorize an inner Repeat merely because it is selected within an enclosing Repeat, and do not grant scoped one-play authority.
- Semantic command/macro resolution: crates/deadpan-core/src/semantic/planner/framing.rs:160-188 builds Shot for the selected owner using node kind and currently admits Source/Hold/Sequence. It must admit the same direct Repeat case, with selection scope validation; the CLI reaches this shared Core planner. Test named replay and dot, as native Keep creep is recorded as semantic Zoom intent.
- Core builder regression coverage: crates/deadpan-core/src/framing_zoom/tests.rs:503-560 already asserts both live/fallback scale bases for Source/Hold/Sequence and refuses ranges. Extend to Repeat and retain the existing refusal case with no ordinary owner.

## Why the plan appears reusable, and what still needs proving

- crates/deadpan-plan/src/plan.rs:1469-1494 evaluates each node’s authored framing with that node’s local coordinate and compiled extent, and stores PictureFraming { instance, local_position, duration, effective_duration, pose }. A Repeat’s local coordinate/duration are its full repeated output clock, not child-play local time.
- Repeat traversal at plan.rs:1654-1715 locates the play from Repeat-local position, moves local to the selected play/gap position, then enters the child. Authored Repeat framing stays in the outer picture stack; plan.rs:1300-1360 resolves follows from provider picture source time while transition scale was evaluated from Repeat-local owner time. Existing Sequence evidence already exercises a transitioning outer owner across changing source providers, but not Repeat placement/gaps.
- No new framing representation appears necessary: FramingClock::OwnerOutput and FollowScaleTransition factor/curve already model this. Keep existing provider target mapping, Lost retention, clipped-target fallback, and Hold captured-context projection.

## Repeat-specific risks and decisive witnesses

1. **Repeat and child clocks:** use at least three plays, with target motion during each play, so scale’s owner progress is continuous across the Repeat while the target center follows each play’s mapped provider time. Verify frame centers at play starts/ends and exact repeat terminal, including Q32 scale independently.
2. **Default and owned gaps:** Repeat traversal resolves a default gap at the owning play’s gap location and inserts escalation for that play before resolving the gap (plan.rs:1673-1715). An explicit gap Hold is a child and gets its own nested framing. Check Follow transition continuity in both gap types; Hold geometry/captured projection and Lost/fallback behavior still apply. Do not let gap insertion change owner duration or normalize creep per play.
3. **Per-play escalation interaction:** RepeatEscalation::pose at crates/deadpan-core/src/repeat_escalation.rs:134-158 creates a per-play centered zoom; plan.rs:1684-1710 adds this as a distinct PictureFraming layer on the Repeat and can add its own Follow zoom target. The Follow resolver processes layers from inner to outer (plan.rs:1300-1360). Add a case with existing zoom-step escalation and a case without it. Prove the outer Follow center remains correct after inner escalation, escalation clamping is still respected, and live/fallback transition scale combines only according to existing layer order. Do not silently merge escalation into or reset the Follow transition.
4. **Occurrence authority:** test a directly selected Repeat in an ordinary Sequence is admitted, but the same Repeat selected as a single enclosing occurrence, an inner Repeat below another Repeat, and any Visual range are refused before history mutation. Repeat node kind alone is not sufficient authority.

Recommended minimal evidence: Core/plan synthetic 3-play Repeat with moving target, one default gap and one explicit gap Hold; scale oracle + target-time oracle at every output picture; audio plan identical before/after. Native replay on a small Repeat verifies Keep creep, Camera preservation/release, one Undo/Redo, refusal for selected play/range, and named replay/dot. Then reuse the120-picture CFR Original and author a compact full Repeat with an explicitly measured final duration: verify every emitted picture, unchanged complete PCM/gain bits at zero offset, and portable copy/history. The media case proves that fixture’s owner and gaps only; it does not qualify tracking accuracy, listening, arbitrary nested repetition depth, or all gap/escalation combinations.

## Reusable existing seams

- Core transition numerics: crates/deadpan-core/src/framing.rs:427-461 and tests :709, :769 cover endpoint curves, fallback exactness and non-grid integer oracle.
- Repeat structure/scale: crates/deadpan-core/src/repeat_escalation.rs:94-159 plus plan traversal above.
- Native admission and camera draft: preview/camera.rs:853-880, :1219-1240 and :1400-1410.
- Shared semantic path: crates/deadpan-core/src/semantic/planner/framing.rs:160-188.
- Current sequence follow-creep evidence helpers: crates/deadpan-cli/tests/preview_export/follow_creep.rs and follow_creep_export.rs; native replay counterparts are under crates/deadpan-app/src/preview/harness/zoom/follow_creep.rs. These cover the owner-transition, no-change/Undo/Redo, semantic replay/dot, all-picture and audio-preservation patterns, but need Repeat-specific fixtures and scope witnesses.

No repository edits, builds, tests, runtime launches, or Git operations were performed for this map.
