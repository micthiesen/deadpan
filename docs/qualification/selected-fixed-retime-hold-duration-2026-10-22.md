# Selected-play Hold duration through one fixed Retime

Status: implemented, independently reviewed and qualified on this Mac within
the narrow family below. No requirement or gate is newly complete.

Selected body Holds can cross exactly one fixed, unclocked Edit Retime below
every enclosing Repeat, with Sequence ancestry otherwise. Either Repeat axis
can supply the selected Plays; other levels retain literal Default or Play
authority. Shared node IDs do not collapse distinct selected families. Existing
contexts, scalar commands and actual staged isolation proofs remain authoritative.
No wire, schema or dependency change is required.

Every member, including a no-op, is validated before allocation. Fixed crops,
pitch and output extent survive each setter. Final primary continuation uses
the sampled cropped interval. Hidden or unsampled live counterparts refuse;
unused Default definitions retain no invented presentation. Other temporal
placements, Retime-owned gaps, Partition, derived clocks and unsupported sounds
retain their guards.

## Public old-code control

Base `c424c8732874e37b37c335962849ae6205d1d23e`. The silent 42f fixture selects
Outer Play 2 and Inner Plays 1 and 2. Public many-target Rename isolation needs
node pools [11,5], no marks and three actual proof steps. Both targets initially
share one Hold ID, then map to distinct owned Holds. Raw Compound setters grow
both Holds from 2f to 4f while keeping every fixed stage at 6f and the project
at 42f. Exact primary visibility changes [33/2,18) to [33/2,39/2), sampled
[16,18) to [16,19); the second changes [45/2,24) to [45/2,51/2), sampled
[22,24) to [22,25). All four other uses, exterior pictures, original templates,
inverse patches and JSON round trips pass before the sole final positive
availability assertion fails in 0.015s.

The baseline freezes 1,957 Rust files. A Git-blob audit proves 1,955 base files
unchanged, with only the witness registration and new witness different.
Executed app test SHA-256:
`a59df10acc2e88edb3f71ab5b5960e0e189b68e6ed8f376e1a18364defc7f3f5`.

## Undo ownership regression

The first focused run passes eight checks and fails three. The prior no-op
refusal fixture used a now-eligible Edit Retime; it now tests Partition without
relaxing the pre-allocation guard. The export fixture incorrectly expected a
successful no-op plan; it now requires the explicit no-edit error, no allocation
and unchanged durable history.

The Store failure is a real Undo collision: two automatic extension intents
born on isolated Holds map backward onto one shared Hold, violating the unique
physical-owner constraint. A bounded LLDB run locates the transition in Undo;
optimized Rust locals are unavailable. Its four Store production source files
match the pushed base, and the failing binary and source snapshots are retained.

Inverse current-intent mapping now requires the complete canonical authored
address in the restored document. A removed clone's intent keeps its cloned
address until normal reconciliation closes it as Deleted. Historical request
and preparation addresses still map. Writer admission and chronological replay
share this rule, with no weaker uniqueness constraint. The regression checks
unchanged original authority, Deleted clone terminals, full replay, reopen of
the undone state and fresh Redo authorizations. General and authority follow-up
reviews report no findings.

## Native replay findings

The first replay attempt ran zero scenario checks because its CLI allowlist
registration was missing. The next attempt reached the no-op check, whose test
incorrectly expected an error field instead of the correct unchanged-state
notice. Both failed attempts retain their frozen sources and binary hashes.

After correcting that assertion, both inner-selected workflows passed, but
entering an inner scalar scope cleared the outer selected set. The native
report retains 115 passing checks before the resulting missing-pair exception.
Scalar scope navigation now preserves fresh ancestor selections; choosing a
new multi-play axis replaces the old one. Receipt-mapped sets still revoke on
scope navigation. This deliberately narrows the previous UI rule that any
scalar scope choice ended every selected set. Candidate counterpart resolution
also runs before committing either scope switch, so heterogeneous owned branches
refuse atomically. Four focused model regressions cover scalar retention, axis
replacement, mapped-receipt revocation and complete navigation-state preservation
after a structural mismatch. The final source review reports no further findings.

The first complete debug replay (debug-visual-attempt-4, source stage
regression-attempt-1) passes 219 workflow checks across all four scope
combinations and 2,298 frames, plus the Kestrel audit. It retains the screenshot
budget warning, 123 second-retry frames and nine consecutive-retry runs with
distinct causes. Checks confirm no painted frame ignored a retry and no
identical retry repeated for three frames without new input. The final debug
replay after the diagnostic correction also passes all 219 checks, across
2,323 frames (debug-visual, source stage regression-final).

The ordinary member preflight still uses the shared indexed ancestor map.
Only its rejected fixed-path case runs the additional fixed-window preflight.
Fixed-path validation can still rebuild graph geometry for each member; no
large-selection latency qualification or measured speedup is claimed.

## Broader regression findings

The first broad run completes all 3,528 tests in 228.349s: 3,525 pass,
three fail, four are slow and one passes with a leaked-output-handle marker.
All 1,962 Rust files, 153 executed test binaries and four helpers retain their
pre-run hashes. Its failed candidate is retained separately.

One new model test incorrectly expected its retained entry cursor to follow
the visible projection; the native caller already does that, as the passing
replay verifies. A prior refusal test still rejected the now-supported selected
fixed-window family; it now compares the actual primary interval with the
existing shared-window control. The third test exposes a diagnostic regression:
the fixed-path fallback replaced a forged Play/gap ownership error with a
generic target-validation message. Invalid scoped targets now retain the
ordinary diagnostic; valid fixed paths keep their own geometry/clock errors
and limit failures. Authority follow-up confirms this decision table.

The corrected broad run passes all 3,528 cases in 232.819s, including all
11 original focused cases and the four new scope regressions. It reports three
slow cases and one passing output-handle marker, on a different pure command
test. The earlier marker's model test passes without a marker in this run.
All 1,962 Rust files, 153 test binaries and four helpers retain their hashes.

Strict Clippy then identifies one redundant clone of LookupStats, which is Copy,
in the new app test. The retained test-only delta proves every other Rust file,
including all production source, identical to the passing broad candidate.
After that cleanup, all 211 affected normal-app cases pass in 15.439s and all
213 UI-feature cases pass in 15.172s. Both suites retain complete pre/post source,
test-binary and helper hashes; neither final app rerun reports a leak marker.
Both earlier marker cases also
pass isolated exact runs of the retained normal-feature binary, returning
stdout/stderr EOF within the 30s deadline. Strict all-target checks, UI Clippy,
formatting and whitespace checks pass.

## Focused verification

All 11 focused tests pass in 102.540s, including the 16-case debug export test;
one test is slow and 5,282 are skipped. All 1,962 Rust files, five selected test
binaries and four runtime helpers match their pre-run hashes. Locked dependencies
retain their original checksum. All-target checks pass. Three initial independent
source reviews report no findings; runtime evidence exposed the failures above.

The core matrix covers both axes, either other-level scope, both primary orders,
three pitch policies and two crops: 48 combinations with fresh growth and owned
shortening. Pools are [5,5], [11,5], [6,6] or [11,11] according to the captured
families, then zero. Partial no-ops, counted setters, hidden or unsampled no-op
counterparts, dormant defaults and late pool/visibility failures are covered.

The nine-stage Preserve fixture checks every one of 11,212 input cells and
suppression values against a literal 44.1 kHz phase oracle, for all actual mapped
instances, both axes, Default/Play choices and prebound/unbound inputs. Entry at
frame 16 is exactly 25,625.6 root mix samples, with PointCeil 25,626. Chunk parity,
finite support exhaustion, complete unrelated output intervals and the exterior
remain checked at fixed 40f output.

The accepted-provider Store fixture retains its independently owned sibling and
108f output. Nested saved calls commit four changed leaves as one Compound.
Shortening preserves admitted media; extending past coverage creates fallback
and one final preparation per selected target. Immutable accepted controls and
original request identity remain separate. These are synthetic admission/history
cases and do not qualify model inference.

Sixteen debug exports verify 1,312 pictures and 2,101,304 presented samples
across 40 complete zero-offset windows without failures or flags. Unit cases
compare nonzero 256-sample suffix probes at exact old/new origins for all nine
uses and the exterior, within 1e-6. The affine inner-Default growth case retains
a 12.007609dB local-block SNR minimum at sample 108,480; its complete window
measures 35.555287dB. The existing block threshold remains 1.5dB. Listening and
full mastering remain open.

## Release native workspace

Both release native modes pass 219 workflow checks plus the live Kestrel audit
of 62 reserved bindings and 21,885,504 routing cases. Visual mode paints 2,397
frames; performance mode paints 2,618. Quiet input-to-state p99 is 1.66475ms over
652 samples, with no failed or timed-out state samples. This is the retained
two-target workflow on this Mac, not large-selection latency qualification.

All four final 960x640 captures were inspected. Both literal Repeat scopes,
the selected-play explanation, duration entry, separate clocks and footer keys
remain readable. Each mode retains 123 second-retry frames and nine runs of
consecutive retries with distinct causes; visual mode also reaches the
intermediate screenshot budget. The guards for ignored painted retries and
repeated retries without new input pass. Full raw findings remain in evidence.

Executed release app SHA-256:
`f9b1b935359224498e888c6b366c7ee310a45874a5fc4473ac142f82fddcdc29`.
App/test binaries are rebuilt for their recorded stages; unchanged runtime
helpers are retained under their own hashes. A build does not establish
clean-machine or acoustic qualification.

## Release outputs and retained evidence

The release 16-case export test passes in 96.503s. Every one of 1,312 pictures
and 2,101,304 presented samples passes emitted-file verification across 40
complete zero-offset audio windows. There are no failed pictures/windows,
nonzero offsets, unobservable audio windows, uncovered samples or flags.
The reports retain 1,392 observable picture-neighbor comparisons and 1,200
unobservable repeated/frozen neighbors.

The release affine inner-Default growth case again measures a 12.007609dB
local-block minimum at sample 108,480 (2.26s), with 35.555287dB over its complete
window. The 1.5dB block threshold is unchanged. The independent unit probes and
affine phase oracle remain separate from encoded-file alignment; no event-based
realignment, packet dropping or AAC-block tolerance is used.

Five owner archives retain all four 91f unit-growth scope forms and the 73f
affine inner-Default growth case, nine files each. Safe extraction and full
file hashes pass. Each extracted package returns valid=true from project
validation; doctor reports ten history revisions verified by their receipts.
Full chronological replay is separately covered by the Store regression.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-22-selected-fixed-retime-hold-duration/inventory.json)
retains all logs, reports, identities, four inspected captures, sixteen uniquely
named MP4s and five owner packages. Thirteen frozen Rust stages deduplicate
into eight completely rehashed source archives, including failed candidates.
An independent final audit rehashes the inventoried evidence, every source-stage
mapping, all sixteen MP4s, all 45 owner files and four captures without findings.
Its receipt is retained with the evidence.
The final app/UI stages and release source match all 1,962 current Rust files;
the explicit test-only Copy cleanup is distinguished from the earlier broad
candidate. Cargo.lock retains SHA-256
`1e39d18ae4de6c7b898edad39eb7104235fb3ffe3af84521ca94df2b6c25b71b`.

Measured here on Mac17,7, M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1
and the pinned FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 qualified-3 prefix.
Physical keys, VoiceOver speech and listening remain in the owner list.
Named-mark semantic selectors, remaining temporal families, sound owners,
full mastering, renderer work and complete workflows remain open.

DP-06 and DP-07 remain partial. No requirement or gate is newly complete.
