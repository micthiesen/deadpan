# Composite suffix pause insertion, 2026-09-26

Core 24/database 30 extend `InsertTime` at existing root Sequence seams before
composite suffixes. One transaction inserts the Hold, captures current placement
steps, moves content-following marks and retains exact history. Earlier admitted
Source/Hold paths keep their previous reducer output. The [contract](../INSERT_TIME.md)
and [splice design](../STRUCTURAL_SPLICE_DESIGN.md) retain the remaining full scope.
No DP requirement or delivery gate is complete.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-26-composite-insertion/README.md)
includes raw gate and focused logs, source hashes, public-command traces,
independent reviews, the failed Metal replay, and the paired latency diagnostic.
Git metadata is read-only in this session, so these changes are not committed or
pushed. The complete source and concurrent UI harness are preserved in the task
checkpoint for an authorized Git-writing session.

## Behavior and compatibility

- Current root-clock templates are captured for bound and unbound physical and
  default-gap owners. Existing lattices, resume terms and chronological steps
  remain intact. Each shifted owner gains one windowed step without expanding
  Repeat plays or sparse branches.
- Movement stops at nonunity Preserve outputs; input preparation history keeps
  its own clock. Current recipes remain editable independently of retained
  timing, including isolated RoomTone gaps and later default-policy changes.
- Root marks apply insertion bias once. Concrete occurrence and source anchors
  retain their own coordinates; sequence pins and unresolved marks stay intact.
- Database schemas through 29 check the frozen contextual admission before
  modern replay. A valid current composite history cannot be relabeled as old
  history even when its snapshots and patches agree with each other.

The genuine database-29 fixture was produced by a preserved core-23 CLI, SHA-256
`7a02df4266800123c60440cb8f78450bb5b46a9c7e5608f21a7b14cfc29645fd`.
It contains 16 revisions, eight edits and a pending redo, including old-admitted
InsertTime, gap isolation, direct/occurrence overrides and undo/redo. Its
[producer](../../crates/deadpan-store/tests/fixtures/produce-v29-composite-insert-history.py)
and [provenance](../../crates/deadpan-store/tests/fixtures/v29-composite-insert-history.provenance.json)
retain exact input, output and command-log hashes. Migration compares old request
and patch JSON byte-for-byte and preserves the backup on refusal.

## Focused verification

- Capture: eight lifecycle tests and six existing integration tests passed.
  New cases cover existing bindings, current aliases after Split, mixed owner
  clocks, billion-play lexical paths and aggregate capture admission.
- Core InsertTime: 15 tests passed, including atomic inverse/JSON round trips,
  root/occurrence/source mark behavior and one compact step per owner for a
  billion-play Repeat.
- Picture/gap plans: four tests passed. An actual Split through a sparse gap
  branch followed by insertion preserves every suffix picture, stable occurrence
  and lower framing clock; the root clock alone lengthens.
- Decoded PCM: three new tests passed. Actual Split and repeated InsertTime
  preserve the independent NTSC `1602` entry and `-1/5` phase oracle. Isolated and
  default RoomTone gaps retain distinct content through growth and live Silence
  edits. A cropped Preserve output retains its full 384-sample preparation
  history and rejects a 383-sample preparation allowance.
- Migration: all 84 tests passed, including valid current histories deliberately
  stamped as database 27, 28 and 29 and required to fail before replay promotion.
- Native project service: all four pause tests passed, including measured VFR
  freeze/framing, repeat-seam insertion, explicit interior refusal, reopen and
  undo. These tests do not create a native window.
- Durable public commands: 44 `deadpan-app --headless` invocations passed across
  11 distinct revisions and six history entries. The script creates its fixture
  through commands, isolates a gap, Splits a Repeat, inserts two pauses, verifies
  exact undo/redo after reopening each process, and rejects an unsupported
  interior without allocating history. The tested app binary SHA-256 is
  `77a06d877aab6d97d7a447b152ec12f048a2b3b2be0a0e17db7abf3256e5c0ac`.

## UI harness and remaining verification

The completed concurrent `ui-harness` remains integrated. The `editing` scenario
now submits `:hold 11f` before a Repeat, checks duration, cursor and selection,
and undoes only that pause through production input routing. The attempted
visual run failed before rendering with
`CustomNativeAdapterSelectionError("No adapter found")`. The independent
Kestrel audit passed 3,472 routing cases against 62 reservations. No new editing
capture, visual assertion or physical-display evidence was produced.

The earlier [host UI qualification](ui-feedback-2026-09-26.md) retains its nine
successful scenarios and its failing Repeat/Hold latency budgets. It does not
cover the new composite pause replay. Release timing and native GUI review of
that new path remain unverified because Metal is unavailable in this session.

## Retained preview latency diagnostic

The completed harness also provides enough trace detail to narrow the earlier
host-run latency investigation without another GUI session. A derived script
pairs each edit's commit, request, received picture, submission and completion
by command order and ticket. It excludes the four warm-up edits and all undo
operations, verifies all 40 pairs against the independent reported metrics, and
checks the source report SHA-256
`95a4994a039c79912b16e2f174872507502fd9cf2f6ed0bbf4df42101426079f`.
It does not subtract unrelated percentiles.

| Paired interval, p95 milliseconds | Repeat | Hold |
| --- | ---: | ---: |
| Observed commit to picture completion | 71.79 | 72.30 |
| Commit to picture request | 0.063 | 0.137 |
| Request to decoded-picture receipt | 67.77 | 69.03 |
| Receipt to GPU submission | 1.75 | 1.65 |
| Submission to offscreen completion | 2.42 | 2.47 |

These intervals partition each individual sample; their percentile values need
not add. Most post-commit time in this run precedes receipt. That measurement
combines worker preparation, decode, queueing and UI observation, so it cannot
identify one bottleneck. Changing picture tickets across edits does not itself
prove source reopening. The original release binary identity, full compressed
source report, derived script and every matched sample are retained separately
from the new increment's verification. This is analysis of earlier evidence,
not a new runtime or physical-display measurement.

The trace also contains long gaps between harness frames. For the first Repeat
warm-up, the requesting frame ends at 519.979833 ms and the next frame receives
the picture at 579.941250 ms. Both timestamps use the same `Driver.started`
origin. The 59.961417 ms gap does not reveal when the worker finished. The
driver waits with `sleep(1ms)` between steps; that requested duration is not an
observed sleep duration. The next diagnostic should distinguish actual sleep,
worker publication and UI consumption before changing media code or attributing
the earlier misses to decoding. Current project decoder keys exclude revision
and presentation tickets, so those ticket changes alone do not reopen a source.

A separate optimized Rust probe used the same `std::thread::sleep` API for 200
requested 1 ms sleeps in this sandbox. Observed p50/p95/max were
36.77/63.66/67.37 ms; 159 exceeded 16 ms and 64 exceeded 50 ms. A bounded
producer/consumer channel with acknowledgment, measured immediately before send
through blocking receipt, had p50/p95/max 0.0031/0.0438/5.62 ms over 200 samples.
No channel delivery exceeded 16 ms. Probe source and stdout summaries are retained;
this small diagnostic did not retain individual timer samples.

Workspace compilation ran concurrently; scheduling and power state were not
controlled. This separate process does not reproduce the historical replay or
measure its decoder. It does establish that a requested 1 ms polling sleep can
dominate observation latency in this environment. Repaint/service wake-driven
waits and worker-publication timestamps are required follow-up before attributing
the earlier harness timing misses to expensive application work. The recorded
misses remain actual replay observations, with this newly measured limitation.

## Repository checks

Three independent reviews covered general correctness/integration, exact audio
timing/compact admission, and migration safety. All returned no findings. The
review base was the preserved pre-increment core-23/database-29 source snapshot,
so earlier reviewed work and the concurrent harness remained intact.

The stable full gate covered 490 source/configuration paths with unchanged hashes:

| Check | Result |
| --- | --- |
| Workspace formatting and all-target Clippy with `-D warnings` | Passed. |
| Locked workspace tests, including doctests | 1,548 passed, two failed, none ignored. |
| Locked workspace build and doctor | Passed; core 24/database 30 reported. |
| Optional `ui-harness` all-target Clippy | Passed. |
| Optional app feature tests | 180 unit and two integration tests passed. |

One failure was an old gap-binding test expecting root-seam insertion before a
Repeat to be rejected. The assertion was updated to check successful seam
movement and continued refusal of Repeat interiors. This is the only source
change after the stable full gate; its focused validation is retained separately.
Formatting, core all-target Clippy and all 36 audio-binding integration tests
passed afterward. Production sources remained unchanged. The original failed run
is not relabeled as passing.

The other failure is unchanged: `deadpan-jobs/tests/artifact.rs:200` cannot bind
its test Unix socket in this sandbox (`PermissionDenied`, OS error 1). The test
was neither weakened nor skipped. The complete repository gate therefore remains
failed. Missing Metal separately prevents the new visual replay and performance
run described above.

The native lifecycle smoke check is not repeated because this change does not
alter startup or shutdown. Arbitrary Source interiors followed
by composite suffixes, nested/fractional insertion, selected Original moments,
generation scheduling/acceptance UI, mastered playback and export remain required.
