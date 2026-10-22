# Selected-play Hold duration semantics, 2026-10-21

Verified on this Mac. No requirement or gate is newly complete.

## Authority and execution

One invocation captures 2..=1024 Hold bodies, retaining a primary target,
optional primary presentation and ordered counterparts. Their explicit Play
choices vary at one common Repeat axis under an identical earlier prefix.
Later Default/Play choices remain explicit. Sequence/Repeat body ancestry is
supported; owned gaps, Retime/Partition ancestry and placed sounds keep their
existing refusals. Generic framing authority remains separate.

The duration-only runtime context is boxed. Saved programs still contain only
`SetHoldDuration { length }` and named calls, without captured node or play IDs.
Every target is validated before allocation, including unchanged members.
Valid no-op members skip isolation, and all-no-op requests make no history.

Each changed member gets a distinct revision and exact staged isolation pool.
Mixed targets use the existing `SetScopedHoldDuration`; explicit all-Play
targets use the existing occurrence duration reducer. Actual execution proofs
map every target and the primary presentation after each leaf. The complete
Compound is checked under existing aggregate bounds and commits once. The
primary boundary is resolved after all resizes. Opposite length changes can
produce a real edit with zero root-duration delta.

The native receipt independently maps the original capture through the final
Compound proof, verifies primary endpoint pictures and returns every mapped
counterpart. Inspector hints use cached ancestry rather than whole-document
validation during painting. Apply, recording, named calls and direct-edit dot
retain invocation-relative duration intent. Delayed receipts cannot reclaim
navigation after an away/back roundtrip.

## Valid old-code witness

Base: `f384a85810f51be4d52ebfbc86b24071ef111268`. Only the new native test and
registration were installed. The final public `State::hold_parameter` assertion
fails with `None` instead of `Some("hold-duration 2f")`. One test fails in
0.240s, 1,207 filtered. The executed and retained app test binary SHA-256 is
`7ff5e90be71194e62cb4d46ec12fb22bf460432bfc1d6fcc8738c0ed367ce463`.

Before that assertion, the test prepares an actual qualified offset-bframes
Original, Freeze and RoomTone, captures outer Default/inner Plays 1,2 and a real
primary presentation at Edit 19, then runs existing public scalar mixed edits
on an immutable copy. Exact node pools 4/0 and staged lengths 45→47→45 prove the
selection is valid even though its final total length is unchanged. Both
recipes become 3f, the primary ends back at 19, and the counterpart moves 25→26.
The full actual Compound proof, independent owners, exact inverse, JSON and
unchanged service workspace pass. Generic framing and scalar mixed batch
authority still refuse. This is an entry-discoverability witness, not a claim
that the old service already implements batch duration.

## Reviews and retained corrections

Three independent reviewers covered general integration, temporal correctness
and verification. The general review found the cached hint walking root-to-Hold
while consuming Repeat steps in reverse. The hint now follows outer-to-inner
order. A verification finding requested a third selected target; its regression
retains order 2,0,1, exact pools 13/4/0, three distinct final Hold IDs and the final
primary boundary after two earlier-in-time changes. A constant-size authored
graph separately checks query/wire admission at 1024 and refusal at 1025 or zero
counterparts. The temporal review found no material issue.

Initial integrated compilation found four store-test API errors: an infallible
constructor used with `?`, usize instead of u32 iteration positions and an extra
reference in a provider assertion. Later setup corrected a private CLI error
constructor, explicit-path audio child-module resolution and standard Range
field access. These attempts ran no behavioral tests and are not baselines.

The first runtime pass passes 14 of 15 tests in 23.019s, including the actual
native service, both PCM tests, per-target accepted providers/history and real
encoded output. The third-target fixture fails before planning because raw
`SetRepeatPlays` requires a Sequence parent while its inner Repeat directly
belongs to another Repeat. The corrected fixture uses public compact
`IterationOrder::resized` to author its initial three-play order, preserving
the first two IDs and deriving the third. Production command scope is unchanged.
The failed binary/source and a direct exact-binary reproduction are retained.

The native replay oracle was also corrected before execution: a common outer
Play copies 9 nodes, and its explicitly selected inner Play needs 4 more to
preserve that copied Repeat's literal default. The oracle checks 13 copies and
the unchanged 2f literal default. Dot cleanup undoes its two edits separately.

The first release visual replay reaches 431 scoped checks, then an existing
selected-Source recording refusal fails its guidance assertion. Recording,
registers and authored state remain unchanged, but the new multi-target capture
route reports a Hold-duration error for selected Source beats. The route now
requires the primary node to be a Hold; selected Sources retain the established
single-play guidance. The old refusal assertion is unchanged. Its failed
report and release binary are retained separately from the final replay.

The second visual replay reaches 571 scoped checks and fails exactly one obsolete
several-play refusal assertion. The command succeeds, reporting `Pause length
set to 7f across 2 selected plays.` That old assertion now checks supported
Enter-prefill with no isolation or writes; the dedicated selected-duration
scenario separately checks actual commits, mapped targets and history. The
second failed report, executable identity and pre-correction source are retained.

The third visual replay reaches 942 scoped checks, then the first new selected
capture fails: changing the selected play set returned the inspector to its
body Sequence. The replay now re-enters that Sequence and selects the Hold.
Before the next execution, inspection also corrected reopened navigation to
expect the mapped Hold ID, the cloned literal-default oracle to inspect its
three-Hold Sequence, and the delayed oracle to distinguish one new Hold from
the already-owned Hold updated in place. These are harness corrections; the
third failed report and executed source/binary remain retained.
An independent follow-up review checked the corrected keyboard routes, fixture
math, copied default, mapped reopen, macro/dot and delayed-receipt preconditions
and found no remaining source-level issue. Final UI-harness Clippy passes with
no warnings after these harness corrections.

The fourth visual replay passes both direct branch families, reopening, delayed
navigation, recording and named calls, then fails at the dot setup after 1,023
scoped checks. Inspector navigation intentionally revokes mapped multi-target
continuation, as the existing pause-continuation regressions require. The
replay incorrectly expected that set to survive `k`. Its corrected setup
checks the revocation, explicitly chooses a new play set on the preceding Hold,
then applies the saved duration intent. Production selection policy is unchanged.
The fourth report and executed source/binary are retained.

## Final automated verification

All 15 focused tests pass in 23.004s, with 5,201 filtered. Ten core tests cover
zero-net edits in both primary orders, same-node selected families, common
ancestor copies, counted calls, no-op members, partial clocks and mark-copy
loss, dormant/million-play structure, late failures, three targets and exact
selection bounds. The native service test proves real qualified capture, one
revision/history row with two leaves, zero-net authored changes, complete
mapped continuation, stale/no-op/duplicate refusal, Undo/Redo/reopen and nested
counted calls. The store test checks accepted 4→3 retention beside accepted 2→3
fallback on exact mapped targets through history and reopen.

Two canonical PCM tests cover partial historical 44.1 kHz Source bindings,
unbound tails, RoomTone Holds and gaps, both branch families, both primary
orders and grow/shrink. Surviving allocations compare at analytically moved
origins, with independent current-endpoint RoomTone oracles, suppression and
chunk independence. The real-media test checks decoded Source windows and
complete encoded output; release results remain separate below.

Nextest reports one concurrent-run captured-output leak marker on the pure
partial-clock/mark test. The exact retained binary passes alone in 0.021s, with
stdout and stderr reaching EOF before a 30s deadline. Its SHA-256 is
`269945397a2d502372f5d570f861b3b65a09ca3d01388a0ac703fc637f35d3cd`.
The marker is retained; its cause is not established. The command-reference
update intentionally exits with a rewrite assertion; its ordinary check must
pass in the broader run.

All 3,446 broader tests pass in 142.254s, with three encoded-media slow markers
and no captured-output marker. All 198 UI-feature tests pass in 15.410s, without
markers. The ordinary generated command-reference check passes in that broader
run. `cargo xtask check --since f384a858 --no-test` and explicit UI-harness
Clippy over all targets pass with no warnings. Formatting passes.
Those broad results precede only the final Source/Hold capture-kind guard.
After that fix, all 196 affected normal app tests pass in 14.981s. Final
UI-feature follow-up passes all 198 tests in 15.260s. Repeated changed-package
checks and explicit UI-harness Clippy pass without warnings.

## Native release verification

Both final release visual and quiet performance replays pass all 1,318 scoped
checks plus the Kestrel shortcut audit. They retain 8,526 and 8,798 semantic
frames respectively, below the existing 9,000-frame bound. The executed app
SHA-256 is `2cb31657a789e66aae1f58fd8722690fcf1ceb5fb4946b0c3c84076a2786981e`;
all four actual app/helper executables were copied and hashed before execution.
No Cargo process was active during the performance replay, and AC power and
the compared Kestrel source identity are retained.

Quiet input-to-state p99 is 1.418 ms over 1,847 samples; input-to-commit p99 is
25.445 ms over 121 samples. Maximum picture completion is 9.104 ms over 716
samples. There are no failed or timed-out timing samples. These are measurements
of this replay and fixture, not full-workflow or other-hardware qualification.

All three final 960×640 captures were inspected: outer Default and outer Play
entry show both selected inner plays and `hold-duration 2f`; saved feedback
shows 3f across two plays while the total stays 45 frames. The picture remains
dominant and command/footer keys remain readable without overpaint. Existing
Room tone inspector scrolling remains an explicit replay step. The visual run
caps intermediate screenshots while preserving named checkpoints. Both runs
record 354 second-retry frames, and 57/56 runs of consecutive retries with
distinct causes. Their full reasons remain in the reports; no ignored retry or
repeated-identical-cause failure passes silently.

## Encoded release verification

The exact selected-duration release test passes in 19.701s. Four retained
packages export selected bodies under an outer Play and an outer Default,
each growing and shrinking. Their 64/56/68/52 pictures all pass, for 240 total.
All 384,384 presented audio samples pass in two contiguous windows per export, with
measured offset zero, no uncovered samples or flags and minimum block SNR
29.924 dB. All 280 observable neighbor comparisons pass; 192 comparisons are
explicitly unobservable across identical pictures. Independent surviving
speech-window comparisons use analytically moved origins with a 1e-6 bound.
The actual release test/CLI/helper hashes, complete verifier reports, package
file hashes and emitted MP4 hashes are retained. No event-based alignment or
encoder-block timing tolerance is used.

## Evidence and remaining scope

Scratch: `/tmp/deadpan-selected-hold-duration-next-2026-10-21`.
Committed evidence: [inventory](../../tools/media-qualification/evidence/2026-10-21-selected-hold-duration-semantics/inventory.json).
It hashes retained logs, reports, reviews, source snapshots, fixture/environment
identities, executable identities, export manifests and inspected captures.
Source blobs are deduplicated and compressed. Executables, full private packages
and emitted MP4s remain in scratch with their exact hashes recorded in evidence.

Measured host: Mac17,7, Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84),
Rust/Cargo 1.97.1. Development tests use opt-level 2 and locked dependencies;
native performance and encoded qualification use release. The retained
environment record pins the FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 prefix,
Cargo.lock and both actual media fixtures.

Mixed/selected owned-gap duration, Repeat/Retime temporal combinations and wider
occurrence editing remain separate work. DP-06, DP-07 and DP-24 remain partial.
Physical input, VoiceOver speech and acoustic judgement belong on the owner
verification list with exact steps. Local scripted routing, PCM, decoded output
and native replay evidence do not establish those human-only checks.
