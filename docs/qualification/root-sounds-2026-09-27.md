# Root sound-event qualification, 2026-09-27

This increment persists qualified root-owned sound recipes and mixes them into
the real limited Sequence bus without extending picture time. It is a guarded
backend milestone. Native event placement, nested ownership, structural interval
transforms, custom Hold allowances, creative treatments, acoustic acceptance
and preview/export equivalence remain required. No DP or delivery gate is complete.

## Implemented boundary

Core 29/database 35 add reversible `SetSound` and `DeleteSound`. Events retain
complete natural-rate source mapping, exact selection and independent sample
offset, owned edges and gain, with explicit overflow rejection. Storage validates
the selected revision's admitted receipt and original metadata in the command
transaction. Old history replays through closed adapters without gaining sounds.
The preserved core-28 CLI produced the database-34 fixture, including 47 revisions,
22 history entries, an abandoned Retime branch and pending redo. Its provenance,
commands and reconstructed SQL are retained beside the migration tests.

The sound reader evaluates allocation and source phase directly on root RoundEven
samples. It mixes each sound after its own edges and gain with the existing
continuous Original, then uses one shared limiter. A voice's exhaustion cannot
mute another voice. Silent Holds suppress sounds by default while preserving
their source phase. Cached silent PCM retains source admission. Original and
catalog audition exclude edit overlays through their source-only documents.

Temporal edits and frozen audio-context capture reject events until complete
interval and sampled-history transforms exist. This prevents silent loss or
phase reconstruction while retaining all existing backend preparation primitives.

## Review and witnesses

Independent core/storage review found no transaction or temporal-guard bypass.
Plan/audio review caught two integration mistakes during development: the tape
query still used PointCeil internally, and Original I/O preceded aggregate sound
dependency admission. The corrected reader selects the actual grid and admits
all static work before provider access.

Review also found abrupt sound discontinuities at silent-Hold boundaries. The
combined event/gate envelope now supplies one short fade, preserves explicit Hard
constraints and ignores unrelated Original cuts. The parent caught a fractional
root endpoint being replaced by a synthetic Hard context boundary; rounded
allocation containment now preserves the exact authored endpoint. Independent
focused review found no remaining gate or audio-integration defect.

Real-media tests use qualified 44.1 kHz mono PCM with explicit center layout and
the existing AAC Original fixture. Independent scalar references verify arbitrary
sample onsets, exact source phase, per-voice gain/edges, shuffled and cold queries,
silence, unaffected Original cuts and complete mixing before one limiter. The
limiter witness separately assembles its full context and compares both emitted
PCM and gain. Reopening and undoing restores the no-sound bus sample for sample.
Revoking a fully gated source invalidates its cached limited tile; restoring the
source recovers. Separate synthetic tests prove shared dependency and residency
limits before provider access and release reservations after cancellation/errors.

The actual CLI executable exercises sound dry-run/commit and limited inspection
without picture-duration changes or inspection history writes. Migration tests
check every retained snapshot/transaction, backup preservation, pending redo and
rejection of new vocabulary even as empty/null maps in old records.

## Verification status

Focused core, plan, storage, real-PCM, resource-limit and CLI tests passed during
development. Workspace formatting and strict Clippy passed. The locked workspace
test run recorded 1,104 passes before two worker-supervisor tests failed because
stderr pipes remained open after their process exited. A single diagnostic run
with more detailed assertions passed all 13 supervisor tests without a production
change; that result did not close the failure. The remaining packages then passed
739 tests, and all remaining documentation tests passed. Source hashes were stable
through each run. The worker-launch cause and subsequent fix have a separate
[process qualification record](process-launch-2026-09-27.md).
Its corrected deterministic witness passed, followed by no further production
changes; all 146 affected worker/media and 246 optional app-harness tests passed,
along with their strict lint checks. No failed result was replaced by a green retry.

The first PCM gate-test compile failed on omitted empty Subtree maps; the fixture
was corrected before its successful run. Retained logs distinguish failed
development checks, the failed workspace run and its continuation. No live test
was restarted because an observation yielded or its output was quiet.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-27-root-sounds/README.md)
includes exact command records, compressed logs and source identities.

This backend change does not alter native widgets or keys. No new GUI replay,
device probe or listening result is claimed. Existing design boards and native
interaction evidence remain applicable only to their previously verified scope.

The built-in imagegen tool produced a new
[sound-placement target](../design/boards/sound-placement-board-v2.png) with its
[exact prompts](../design/prompts/sound-placement-board-v1.txt) and
[review correction](../design/prompts/sound-placement-board-v2.txt) retained in
the repository. Visual review preserved picture priority, separate catalog and
event selection, a compact placed-sounds list, explicit focus and visible keys.
The correction replaces a misleading decimal-frame precision claim with exact
48 kHz samples and makes partial suppression by the selected Pause visible.
It is a future native implementation target, not evidence of completed GUI work.
