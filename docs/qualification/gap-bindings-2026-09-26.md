# Authored Repeat-gap audio bindings: 2026-09-26

This increment implements [authored Repeat-gap bindings](../GAP_AUDIO_BINDINGS.md)
in core 22/database 28, using the existing timing records, reanchor expressions,
plan walkers and PCM consumer. Audio-context schema 2 is unchanged. General
atomic Original-moment splice and its native Visual/register workflow remain
open. No DP requirement or release gate is complete.

The checkout includes earlier captured-framing, Original-moment, gap-clock and
compact-reanchor work plus a concurrent native UI harness. The reviews below
cover the gap-binding increment. The optional harness requires separate feature,
visual and performance verification. No new interface board was needed for this
timing/history change; the ten existing ImageGen targets remain authoritative.

## Behavior and evidence

- Gap bindings have their own owner map, recipe discriminator and preceding-play
  argument. Outer Repeat paths retain only outer occurrences. Typed clock scope
  separates a Repeat's full output from its gap definition.
- Capture includes positive configured gaps before they render. Stable surviving
  gaps retain their placement; new and formerly final plays gain canonical gap
  births. Child overrides do not erase the following gap. Nested birth scopes
  keep their own retained geometry.
- Split/isolation copy live ownership while historical aliases stay fixed.
  Aggregate work and identity reservations include both maps. Removing a gap
  prunes its binding; re-adding one does not revive removed intent.
- Current duration, sound source, Silence/RoomTone/Tail policy and edge choices
  control evaluation. A shortened gap can retain a resume anchor beyond its new
  extent, while current support still bounds reads. Unsupported Tail fails
  before source I/O. Explicit silence survives a zero-input-point Preserve map.
- Root, point and physical-domain readers use the same bound PCM path and bypass
  only the selected recipe. Actual decoded 44.1 kHz PCM checks interrupted first
  versus later NTSC gaps, fresh seeks, irregular reads, signed nonunity placement,
  current policy changes, reordered stable plays and preceding-child overrides.
- Headless commands capture an unplayed gap through a real pause insertion,
  grow it, inspect current silence and read the historical RoomTone revision
  using qualified managed originals. Inspection leaves history unchanged.
- Database schemas 1 through 27 replay into schema 28. Frozen core 21 retains
  existing reanchors but rejects the new gap vocabulary; older binding adapters
  also freeze nested clock/template grammar. Tests include empty, null and
  escaped new fields in snapshots and both patch directions.
- An actual preserved core-21/database-27 CLI produced nine fixture revisions
  and five history edits. Only its initial, no-history reanchor snapshot was
  explicitly seeded and validated. Later Split, InsertTime, gap creation/growth,
  rename, undo/redo and pending redo came from the binary. The fixture does not
  relabel schemas or rewrite historical patches.

## Verification

Focused checks passed 102 cases: 55 plan, 12 frozen-adapter, five decoded-PCM,
14 CLI audio-inspection and 16 CLI project-command cases. The full workspace
run passed 1,501 tests, failed four and ignored none. All-target Clippy with
warnings denied, the locked workspace build and CLI doctor passed. The initial
format check saw concurrent app edits; a subsequent formatting check passed.

One full-run failure was `directories_fifos_and_sockets_are_rejected_without_blocking`
at `crates/deadpan-jobs/tests/artifact.rs:200`: the sandbox denied
`UnixListener::bind` with `PermissionDenied`, OS error 1. Its socket rejection
assertion was not reached. Three playback cases hit the existing ten-second
worker wait. An unchanged package-only retry passed those three but timed out
the intentional-panic case with diagnostic backtraces enabled. Finally, the
exact workspace playback binary passed all 11 cases with original settings,
default test parallelism and unchanged assertions/time limits. Its SHA-256 is
`164b98ea42232884dfa15bd5b9f5e90ed2a8f80231eadf1c8984786a8bceb284`.
The original failed runs remain recorded. A numeric system-load observation was
denied by the sandbox, so load is not established as the cause.

The overall gate remains failed. No test was skipped or suppressed.
The gate captured 466 source/configuration paths at entry; ten app paths
changed during verification, including one added file. Gap/backend sources
remained unchanged. This is scoped backend evidence, not a stable whole-checkout
or optional-harness qualification. [Retained evidence](../../tools/media-qualification/evidence/2026-09-26-gap-bindings/README.md)
includes every gate attempt, failed and successful focused runs, hashes,
old-binary command records and independent reviews.

Three independent reviews covered general correctness, timing/PCM and migration.
All closed with no findings. A suspected InsertTime-after-gap migration mismatch
was withdrawn: the old capture walker rejected every positive configured gap.
The actual old binary independently refused terminal InsertTime for both a
three-play and a one-play configured gap, preserving each document/revision.

Initial checks exposed test-construction issues: a test module registered before
its file existed, an obsolete expectation that a gap-bearing unaffected prefix
must fail, a migration fixture/test mismatch and two omitted default fields in
an older persistence literal. They were corrected without relaxing validation.
The first signed-placement PCM expectation used the fractional phase of an
unbound recipe. The corrected oracle pins canonical gap zero to the consuming
grid's allocated anchor, preserving the exact 2/3 step for origin -1/3 and scale
3/2. Root start -534 and PointCeil start -762 at grid origin 1/7 were checked
independently in review. Production timing was unchanged for that correction.

Concurrent UI integration briefly left its manifest/lock metadata inconsistent;
Cargo resolved existing pins offline and verification continued with `--locked`.
The default-feature Clippy run also exposed Dialogs test initializers whose
struct update had no fields left when the harness was disabled. Explicit
feature-gated field initialization preserves both configurations. Formatting
failures during concurrent UI edits remain visible in the retained logs.

## Limits

No device audio, listening, live GUI, physical display or encoded export claim
is made here. Existing image boards establish visual targets, not implemented
Visual/register behavior. InsertTime now permits gap-bearing unaffected prefixes,
but its shifted suffix still requires supported root Source/Hold fragments.
General splice must compose boundary partitions, per-domain reanchors, marks,
picture ownership, identities and generation relevance in one reversible edit.
Movement and raw-recipe lifecycle work also remains explicit in
[the tracker](../REQUIREMENTS.md).

The session cannot write Git metadata. A complete recoverable checkpoint is
retained under `/tmp/deadpan-gap-bindings-20260926/checkpoint`; no commit or push
was performed by this session.
