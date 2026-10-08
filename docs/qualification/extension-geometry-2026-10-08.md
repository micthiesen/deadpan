# Directional extension geometry checks, 2026-10-08

One-sided extensions now have face, mouth and selected-region rejection checks
through the real pinned Vision worker. They use one retained conditioning
anchor. **DP-12 remains Partial:** complete provenance admission, saved Ready
and acceptance, native controls and the longer generation envelope remain.

## Inspection and policy

The native inspection protocol is version 3. `InspectExtensionLandmarks`
carries the complete canonical movie index, one retained PNG, a generated
interval and an optional authored subject seed. It cannot supply a second
boundary. Existing Bridge observations retain their schema and policy.

The worker proves all native PTS values and the terminal end, including context
that it does not analyze. It requires silent, square-pixel FFV1 with the
canonical millisecond clock. It then detects the anchor and each generated
picture. Selected-region tracking travels from the anchor outward: increasing
ordinals for FromLeft, decreasing ordinals for FromRight. Reverse traversal
uses exact bounded seeks, with cumulative decode, packet and I/O limits and
one deadline. It retains one decoded picture at a time. Raw observations are
returned in canonical chronological order, with exact native ordinals and PTS.

Face geometry compares confidently associated generated faces with the one
quiet-pose anchor. Region geometry compares a continuous track with the one
authored seed. Neither constructs an interpolated return path to an opposite
picture. Missing, ambiguous, weak or lost tracking ends the affected track's
coverage; measured rejection survives subsequent loss. Mouth motion uses only
generated pictures in chronological order, including when subject tracking
runs in reverse. All numeric thresholds are the existing Bridge policies.

Input limits are 4,096 total indexed pictures and 1,025 generated observations.
Raw face, landmark and region collections have bounded deserialization.
Context cannot count toward generated coverage. Single generated pictures
remain valid but cannot invent sustained mouth or drift evidence.

`inspect_extension_geometry` copies the private canonical movie and pre-launch
anchor into a fresh worker directory, verifies both SHA-256 and BLAKE3, and
admits observations only after confirmed worker teardown and private output
snapshotting. Its report binds the complete native contract and object, retained
context and receipt, pinned detector/tracker configuration, raw observations
and recomputed policy. No selected target and an unavailable selected target
have distinct reasons. The opposite image stays outside this inspection.

The developer `qualify_extension_media` example now requires an absolute
`tracker` path and runs these checks after pixel checks under the same deadline.
It still cannot grant Ready or accept a candidate.

## Verification

Verification is on this Mac under spec §29.1. The first integrated focused run
passed 173 tests, including real Vision extraction and both tracking directions,
complete context-index/terminal/hash failures, cancellation/deadline behavior,
retained-anchor use, input/report tampering and existing Bridge qualification.
Four final host tests also passed. They create an actual measured Original and
saved subject target, use production extension preparation, and then inspect
the canonical candidate through the supervised helper. Both directions measure
all eight static generated frames and reject gradual generated subject drift.
Selected targets outside their anchor range remain explicitly unavailable.
Deleting worker-writable input copies after retention does not alter inspection.
Seed, policy, runtime, coverage and missing-null mutations are rejected.

Independent reviews covered the face policy, region policy, shared coverage,
native worker/protocol and host binding. The review's selected-target coverage
gap was closed by the real project tests above. Rejection spans use canonical
chronological start plus count consistently in both directions.

The final repository gate passed: 5,348 workspace tests, 1,071 UI harness tests
and two documentation tests, with locked dependencies, formatting and strict
workspace/UI Clippy. Ten workspace and two UI tests were skipped by the existing
gate configuration. Nextest reported pipe-closure warnings on
`scoped_acceptance_isolates_one_play_and_keeps_default_request_current` and
`all_reviewed_physical_reservations_outrank_different_logical_symbols`; both
tests' assertions passed. The cause of those warnings is unproven.

A fresh ad hoc signed, hardened-runtime bundle built in 3 minutes 3 seconds:
730.6 MiB with 74 audited Mach-O files. `bundle-verify --keep` passed every
positive and negative check from a relocated copy in a scrubbed environment,
including project creation, Render, bundled helper and AI runtime probes, and
tampered or missing resource refusals. This is verification on the M5 Max Mac;
it does not establish clean-machine acceptance or packaged extension acceptance.

Source and executable hashes, machine details and compressed command logs are
retained in the [evidence directory](../../tools/model-qualification/evidence/2026-10-08-extension-geometry/manifest.json).
The source record identifies the base revision and every changed Rust source;
the executable record identifies the workers actually exercised and the fresh
bundle's executable files.

Synthetic drawn faces and a moving textured square establish extraction,
direction, coverage and policy behavior. They do not establish identity quality
on real-person footage or calibrate heuristic thresholds. Those judgements
remain on the owner verification list; every generation still needs audition.
