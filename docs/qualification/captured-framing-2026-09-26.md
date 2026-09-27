# Captured pause framing, 2026-09-26

Pauses now retain their input composition independently of the Hold's picture
provider and live Camera framing. This implements the bounded
[captured-framing contract](../CAPTURED_FRAMING.md); it does not complete DP-08 or
any delivery gate. Actual Metal comparison and native visual review of this
increment remain unverified in the current session.

## Implementation and deterministic checks

Core 19 adds an optional Hold recipe context containing ordered canvas fits and
static clipped operations. Native root insertion samples the preceding measured
picture, keeps its exact source PTS, captures descendant operations and excludes
the root's live framing. Even an unframed source retains its canvas placement, so
later aspect changes preserve letterboxing. Recapture compacts redundant identity
clips without removing meaningful crops or explicit identity poses.

Provider changes, generated acceptance, shortening, fallback restoration and
reversion retain the context. Camera edits its own operation after that context;
reset keeps the captured crop. Plans share one immutable context per compiled
Hold or gap. Preview and the offscreen renderer use the same geometry path.

The deterministic tests cover nested clips, all rotations with source aspect,
changed canvases, Fill placement before the first clip, exact VFR frame selection,
parent motion, repeated capture, durable undo/redo/reopen, acceptance lifecycle
and strict serialization. Typed payload tests cover recipes, inserted and override
subtrees, gaps, occurrence edits and patches. Aggregate tests exercise exactly
100,000 records, rejection above it and clearing a repeated occurrence at the
limit. Private isolation intermediates have their own bounded two-budget allowance;
public documents and committed transactions retain the ordinary limit.

The native TargetPicker no longer cycles focus into disabled numeric fields.
A headless egui regression tests Tab and Shift-Tab followed by a target digit or
`f` to return, with the old focus behavior as a negative control. This
does not establish live native focus or IME behavior.

## Actual older project history

Database 25 freezes the core-18 vocabulary. The retained schema-24 SQL fixture
was produced by the actual previous application binary, SHA-256
`4df29f2d65ba76d2cdcf88f4c361a38d416a224270aeae26b3d297052896036e`.
Its SQL SHA-256 is
`7429704f56c886610decc265d100fec07cbfb973455807084534b0fa0a47f3e2`.
It retains eight revisions, four history entries, a framing curve and pending
redo. Migration compares every legacy document and transaction, retains the
backup, consumes redo and persists a new context. Older recipes reject the new
field even when null or escaped; omitted optional gaps remain valid.

## Review and repository checks

Three independent read-only reviewers covered core/storage admission and
migration, plan/render composition, and native capture/keyboard routing.
The storage reviewer found copying before context admission, then an overly
strict intermediate limit that prevented a valid occurrence clear. Both were
fixed, covered by regressions and re-reviewed. No findings remain in those scopes.
Reviewers did not execute tests or GUI checks.

The [evidence directory](../../tools/media-qualification/evidence/2026-09-26-captured-framing/)
retains the review dispositions, source seals, logs, old-binary producer and failed
Metal report. The first complete gate attempt stopped on a Clippy range-expression
warning in a renderer test; it was corrected.

Formatting, all-target Clippy, the locked workspace build and doctor pass. The
next workspace test run passed 743 tests, then failed while creating a Unix socket
fixture in `deadpan-jobs/tests/artifact.rs:200`: `Operation not permitted`.
The failure preceded the tested assertion. Remaining suites and unfinished
doc-tests were run separately without repeating completed suites, and all 617
passed. The worker qualification example's three tests also passed separately.
The combined result is **1,363 passed, one failed, none ignored**. The full test
gate remains failed; the continuation does not relabel it. All 423 source and
configuration hashes remained unchanged across these runs.

Changes remain local and uncommitted because this session's filesystem policy
makes `.git` read-only. Commit and push were not attempted. The overall project
goal remains open, including this increment's GPU and native review obligations.

## Metal and native review limits

The actual offscreen Metal harness built and ran, then failed to find an adapter:
`metal found no adapters`. It performed no pixel comparisons. Existing CPU tests
and older Metal evidence do not substitute for this increment's actual GPU run.

The rebuilt unoptimized native executable is SHA-256
`7ce122ea9973718f98707cabb1a194960e2221b92377cf32eb54ef5bc3bd1fd8`.
Its build followed the unchanged gate source seal. A scratch review package uses
a SQLite backup of the earlier synthetic Camera project and copied managed
originals, leaving the Documents package untouched. Computer Use rejected opening
the rebuilt app with: `Computer Use was not approved to use Deadpan Captured Review`.
No native interaction or screenshots of this build are claimed. The exact pending
checks and fixture identity are retained in `native-gui/review.json`.

All nine existing ImageGen boards and prompt pairs passed hash/dimension checks.
The Camera and single-Original workspace boards remain the visual targets. The
new captured-view explanation still needs visual comparison in a permitted native
session, along with insertion, Camera reset, keyboard picker focus, history and
reopen. No live IME, VoiceOver, physical display, HDR, acoustic, performance or
encoded-export qualification is claimed.
