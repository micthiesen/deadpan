# Native edited-slice evidence, 2026-09-30

See [qualification](../../../../docs/qualification/native-edited-slice-2026-09-30.md)
and the [edited-slice contract](../../../../docs/EDITED_SLICES.md).

`summary.json` records commands, source manifests, test counts, replay results
and environment. Logs larger than 10 KB and full replay reports use gzip.
`manifest.json` hashes all other retained files. Source manifests cover tracked
and untracked implementation inputs; the collector rechecks final gate hashes.

Initial compiler, fixture and oracle failures are retained beside corrected
runs. The initial full release replay's three minimum-picture failures and the
single diagnostic reproduction remain in `replays/` and `images/`. The corrected
compact layout preserves control reserves and hit sizes. Final review found
and verified the saved-refresh message correction; recovery tests retain the
durable success and explicit reopening guidance.

`native/` records the unique temporary release bundle, exact binary identity,
native key sequence and consistent SQLite comparisons. Copy/cancel preserve all
20 tables; placement adds one revision and Undo restores complete authored state.
The native app exits and releases its writer lock. The reserved user window and
its Space were never inspected or changed. Retained screenshots are full Metal
replay captures; native CUA images were inspected with visible-screen clipping.

The full release replay explicitly skips the optional accepted generated-picture
fixture. Its ordinary audio transport is injected, while separate media tests
check decoded PCM. No full native IME, accessibility, listening, AI generation,
large-media performance or release qualification is inferred from these checks.
