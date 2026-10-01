# Atomic MoveRange persistence and headless tests

## Root execution addendum

The parent ran four store tests successfully under `persistence-tests-1` in
67.213 seconds, and the CLI test under `cli-tests-1` in 88.596 seconds. Logs and
source manifests are retained alongside this report. The CLI run used production
JSON ingress and passed all assertions below. Root corrected the ordering of new
module declarations in the two parent files after the first workspace fmt check.
The final locked workspace gate passed all 2,792 tests with none failed or
ignored in 1,399.00 seconds. Formatting and strict all-target workspace Clippy
with the UI harness passed. All final checks share source manifest
`6609d6d091868ae4b71a87a039419bf61209ed4de868f28faf5754e27975e484`,
with all 1,338 inputs rechecked. No native UI claim is made.

## State

Five tests added; owned files formatted and `git diff --check` passes.
Cargo and native UI were not run because root owns the build slot. No production,
core, audio, plan, app, documentation, migration or Cargo manifest files changed.

## Confirmed API

Coordinated with `/root/edited_slice_core`:

- `Command::MoveRange { source_revision, source_parent, range, destination,
  identities: SplitIdentities, timing: AudioTimingId }`
- `MoveRangeDestination::Seam { parent, index }` and
  `Interior { parent, target, at: FrameDuration }`
- `ProjectDocument::range_move(&NodeId, FrameRange, &MoveRangeDestination)`
  returns `SequenceRangeMove`, including required IDs and final inserted range.
- Both source and destination must name the current revision. Historical edited
  copies cannot authorize removal of the current numeric interval.
- Existing root sound recipes and route history retain their unchanged owner
  clock. Move grants no new media admission.

## Files and coverage

1. `crates/deadpan-store/tests/edited_slice/move_range.rs`
   - Five structural/history cases inside one test: whole composite including a
     Repeat, cross-parent in both directions, partial endpoints, and a single
     ordinary Hold needing source In/Out plus a separate destination interior.
   - Read-only preview changes no authored SQL rows. Injected history-write
     failure rolls back the revision and complete document. Commit equals the
     preview and adds exactly one revision and history entry.
   - Serialized stored request equals the original typed request. Reopen,
     fresh-revision Undo, reopen, Redo and final validation retain exact authored
     state. Logical mark IDs and whole Repeat/child IDs remain stable.
   - Separate test rejects stale source, stale destination, insufficient Split
     pool, collision in an unused pool tail, and invalid destination slot without
     writing authored rows. Valid command still succeeds afterward.

2. `crates/deadpan-store/tests/generation_bundles/move_range.rs`
   - Uses the existing synthetic admitted bundle fixture. Explicitly makes the
     old generation request stale while retaining its accepted Hold provider.
   - Moves the complete accepted Hold into a sibling Sequence. Asserts no Split
     IDs, same Hold/provider/assets, one commit, and exact authored Undo/Redo
     through reopen. Request and worker attempt remain unchanged and no current
     request is revived.

3. `crates/deadpan-store/tests/source_registration/sounds/move_range.rs`
   - Reuses real qualified `offset-bframes.mp4`, existing source registration and
     sound event helpers. Creates prior routed sound history with an inserted
     pause, an unrouted sound entirely inside moved picture time, and a separate
     sound with an explicit grant on the surviving silent Hold.
   - Moves the first four picture frames to the end and back. Serialized root
     sound recipes, route journals, and grant maps remain byte-identical. Source
     qualification counts and assets stay unchanged. Checks dry-run, history
     counts, exact authored Undo/Redo through reopen and store validation.

4. `crates/deadpan-cli/tests/project_commands/move_range.rs`
   - Production headless JSON command path for cross-parent seam and a joint
     three-cut interior move, using typed command serialization.
   - Dry-run writes no authored rows; exact preview equals one commit. Stored
     JSON request preserves the typed command and original expected revision.
   - Duplicate/stale destination returns `RevisionConflict`; after Undo a fresh
     destination envelope with an old source revision still fails without writes.
     Unknown destination fields are rejected at JSON ingress.
   - Fresh Undo/Redo revisions, one history row, and exact reopened authored state.

Narrow module declarations were added in `tests/edited_slice.rs`,
`tests/generation_bundles.rs`, `tests/source_registration/sounds.rs`, and
CLI `tests/project_commands.rs`.

## Required root checks

Run under the project's established FFmpeg/native build environment:

```text
cargo test --locked -p deadpan-store --test edited_slice move_range
cargo test --locked -p deadpan-store --test generation_bundles move_range
cargo test --locked -p deadpan-store --test source_registration move_range
cargo test --locked -p deadpan-cli --test project_commands move_range
```

All are currently unrun by this agent. Root may combine test binaries in one
invocation as appropriate. No claim of passing compilation or runtime tests.

## Limits and risks

- Core MoveRange implementation is concurrent and had not yet been compiled
  against these tests. API was coordinated; compile/runtime failures must be
  resolved before treating this as evidence.
- Synthetic Generated receipts establish acceptance/history behavior only.
  These tests do not decode generated video or measure audio/picture quality.
- Root sound tests compare retained complete authored recipes/routes/grants.
  Live gating, sample labels and decoded fractional-rate PCM belong to the
  separate core/plan/audio verification owners.
- Native placement, source refinement, endpoint display and audition remain
  outside this task and are not claimed by the headless tests.
