# Selected-duration review and integration notes

Base: f384a85810f51be4d52ebfbc86b24071ef111268.

The qualified old-code native witness failed only at the final public duration
entry assertion, after the real Original/RoomTone, 4/0 clone pools, staged
45->47->45 frames, exact full Compound proof, changed recipes, inverse and JSON
controls passed. See baseline.json, baseline.log and frozen source/binary.

Three independent reviewers examined the current diff: a general reviewer,
temporal-isolation reviewer and verification reviewer. The general reviewer
found the new cached hint consuming Repeat steps in reverse while walking a
root-to-Hold path. Fixed to outer-to-inner. Root also required hints to use
cached ancestry instead of full document validation on every footer layout.
The temporal reviewer found no material issue. The verification reviewer found
only pair coverage; a third selected-family oracle was requested and added.
Runtime checks remain the authority for all of those source-level conclusions.

Root corrected the selected replay's common outer-Play pool expectation from
9 to 13: 9 nodes isolate the outer body and 4 isolate the inner Play from its
literal default. The replay additionally checks that cloned literal default.
Dot replay cleanup now undoes its two edits separately. A delayed selected
receipt exercises away/back navigation before admission.

Initial integrated compilation ran no tests: four store test API errors
(infallible constructor, u32 iteration ordinals and an extra reference) were
corrected. The second attempt found a private EditError constructor in the CLI
fixture; replaced by the public error fields. A formatting pass also exposed
the explicit-path audio module's child resolution; its sole registration now
names duration/selected_duration.rs. These are setup failures, not old-code
behavioral witnesses or product runtime failures.

The first release visual replay found a selected-Source recording guidance regression. The new multi-target Hold capture now also checks the primary NodeKind::Hold; all 196 affected normal app and 198 UI tests plus strict lint pass after that correction. The second replay reached an obsolete selected-duration refusal after the command succeeded. Its assertion now checks supported Enter prefill without writes; dedicated selected-duration cases cover actual changes. Final replay evidence is tracked separately. A final independent read-only audit requested recording that second intermediate failure; the qualification report now does so.

The third visual replay failed the first new selected capture: scope plays returns to the body Sequence. The new helper re-enters the body and chooses the Hold. Root inspection additionally found that reopen must expect the mapped ID, the cloned default is a Sequence containing three 2f Holds, and delayed execution creates one fresh 3f Hold while resizing an already-owned Hold in place. Worker fixed each oracle before the next execution. An independent read-only follow-up checked paths, clocks, copy pools, macro/dot and delayed preconditions and found no remaining issue. Final UI-harness Clippy passed.

The fourth actual visual replay passed the direct/reopen/delayed/macro cases and failed the combined dot-seed/counterpart oracle after K. Existing revoke_mapped_selection intentionally clears all counterparts on navigation, with pause-continuation regressions enforcing it. The corrected replay checks saved duration intent separately, proves revocation, explicitly chooses the new before-Hold pair, then applies dot. Root additionally corrected its pre-execution ID oracle to use the actually copied before Hold rather than its literal default ID. No production selection behavior changed.

Final independent evidence audit rehashed all 828 then-current inventory entries (15,318,795 bytes), matched all 48 final-source identities to retained snapshots, both replay app identities and 1,318 scoped checks plus Kestrel, performance/timing samples, all four export totals and three inspected captures. No material factual or provenance issue remained. Root subsequently added explicit retained paths to the second/third/fourth failed-replay binary manifests and reverified those bytes, then regenerated and rehashed the final inventory.
