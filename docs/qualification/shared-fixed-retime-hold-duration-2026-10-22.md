# All plays Hold duration through fixed Retimes, 2026-10-22

## Authority and implementation

The existing shared duration authority now admits fixed unclocked Edit Retimes
below its direct Repeat inspector root. Every Repeat choice remains Default;
the separate concrete presentation retains its captured stable iteration IDs.
The ordinary duration command still changes the shared Hold once. Independent
owned plays, fixed output windows and exterior timing retain their identities.
No command, saved instruction, schema or isolation mechanism is added.

Ordinary and shared duration visibility use one private exact projection.
Sequence prefixes translate the visible interval. Each fixed Retime clips that
interval to its input window before mapping it to output time. Each Repeat
uses the current canonical sparse layout for the captured stable iteration,
including independently owned body and gap durations. It never multiplies an
iteration ordinal by a nominal body duration. Project-picture centers select
the final half-open interval: an In tie is included and an Out tie is excluded.
Hidden and unsampled live Holds refuse rather than acquiring a substitute
presentation. A later setter that loses visibility rolls back the whole run.

The complete literal-body ancestry is checked before accepting a dormant
default with no presentation. Dormancy still requires complete body-override
coverage at an enclosing Repeat. A crop hiding a live Hold does not establish
dormancy. This play, mixed and selected timing paths retain their existing
Retime guards. Partition and derived clocks remain unavailable. The native
path retains its placed-sound refusal; the core geometry query does not itself
grant unsupported repeated-sound lifecycle edits.

The native hint walks the selected ancestry from retained indexes. Full core
authority and actual first/last picture validation still happen at capture.
Enter, Change duration and the existing duration command use the same shared
context. Recording stores duration intent independently of node or presentation
identities.

## Valid old-code witness

Base: `ce1568ea7e5e52e259cd3af984087ef55fb2ba53`. Only the new native
model test and its registration were installed before freezing 1,933 Rust
source files. The sole final availability assertion fails in 0.012s, with
1,211 tests filtered. Executed and retained binary SHA-256:
`7ba27d797231dafb8c92da5e5385601f19d786773984091c412f1783da99b528`.

The public fixture is Lead3 / Repeat3(fixed Edit FollowSpeed output6,
input[0,8), child Before2 / Held2 / Speech6) / Tail3. It validates a 24-frame
project with All plays authority and a separate second stable representative.
Raw public SetHoldDuration 2 to 4 already succeeds with zero duration delta,
unchanged fixed stage, Repeat, exterior nodes and branch tables. Exact Hold
start is 10.5, with old end12 and new end13.5; the silent `speech` Hold's
start moves from12 to13.5.
Sampled Hold ranges are [10,12) and [10,13), retaining cursor10. Inverse and
JSON checks pass before the final hint assertion. This is a silent structural
witness, not an audio or media measurement.

An initial unexecuted proposal used private fields, the wrong integer type,
This play to choose a representative and an assertion expecting refusal.
Those errors were corrected before compilation; the original proposal remains
separate in the retained scratch evidence.

## Review and retained corrections

General, temporal and verification source reviews found no actionable issue.
After the root added and executed native recording/Run and delayed-receipt
navigation checks, a final source review and independent evidence audit also
found no actionable issue. The audit rehashed the complete evidence inventory,
all 20 final Rust sources and recorded binaries, and checked the test, replay
and emitted-file totals against the retained reports.

The first real-media attempt used provisional Repeat IDs after insertion;
the fixture now captures its committed stable iteration. The second probe
requested a nonzero window from silent Original32. The exterior is now an
independent copy of the known click-bearing Original29..32, preserving the
nonzero oracle. The third attempt passes both unit-rate cases and affine
growth, then correctly refuses an affine one-frame shrink: its exact visible
interval [0.75,1.5) contains no project-picture center. The positive affine
shrink therefore uses two frames; explicit unsampled negatives remain in the
core tests. No production guard was weakened for these fixture corrections.

The initial integrated check found three new native harness compilation
errors: a private comparison helper, insufficient scenario visibility and a
private structural-duration method. The harness now uses the shared comparison
helper, harness-scoped entrypoint and public node_duration. The next integrated
all-targets check, including the UI feature, passes.

The first focused run passes 18 of 23 in 15.180s. The common visibility helper
returned InvalidCommand when an ordinary capture omitted a Repeat; it now
preserves the existing SelectionUnavailable contract. A dormant Partition
negative first constructed a nonunity Partition, failing document validation
before testing authority. It now uses a valid unity window. The copied PCM
fixture's generic string remapper changed the Repeat enum tag into a node ID;
typed node and override cloning replaces that remapper. A follow-up compile
caught the tuple shape of override iteration, and the Partition repair briefly
shadowed the duration instruction helper. These are retained test repairs.

Two PCM failures selected the wrong physical stage: RoundEven(frame9)=14414
falls before the exact entry14414.4. Input-stage queries now use the first
sample point at or after entry, PointCeil14415, and assert the returned stage's
physical node identity. The complete independent PCM oracle remains intact,
including suppression, one-point exhaustion and chunk parity. All three PCM
tests then pass in 12.468s. Full unchanged middle-output comparisons use their
original root RoundEven interval; only the descriptor query uses PointCeil.

The first ordinary-refusal failure also carries a concurrent captured-output
marker. Its exact retained binary reproduces the same assertion failure alone
in 0.033413s, with output reaching EOF. Its SHA-256 is
`59258f557f71d77519febeccd687241f1a833eedd1b0ef3a45a6ffde15e637d1`.
The marker's cause is unknown. A later concurrent Preserve PCM failure also
carries a retained marker; the corrected assertions pass in the final run.

Root review corrected native harness history checks before execution: Undo and
Redo create new revision IDs, so full authored content is compared separately
from revision. Undo closes the inspector, requiring explicit reselection before
dot. Recording/Run and delayed-receipt navigation are included in this replay.

## Verification

All 23 focused tests pass in 15.811s, with 5,226 filtered. This includes nine new
core cases, the existing ordinary visibility family, native availability,
three independent canonical PCM cases, admitted-provider/history and real
export. One pure core test carries a captured-output marker; its exact binary
passes alone in 0.045234s with output reaching EOF. The full root RoundEven
interval of the independent middle output and exterior tail compare unchanged;
the stage input query separately proves the selected physical owner.

All 3,480 broader tests pass in 148.524s, with 1,769 filtered, four slow markers
on concurrent media work and two captured-output markers. Both marked tests
pass individually from their exact binaries with output reaching EOF, in
0.014096s and 0.207286s. Their causes remain unknown. Final 202 UI-feature tests
pass in 15.207s, with 1,060 filtered. Final changed-package strict checks,
UI-feature Clippy and formatting pass. Broader testing precedes only repairs
and stronger assertions in the development-only replay plus its allowlist.

Both final release native replays pass 60 workflow checks and the Kestrel audit,
in 488 visual and 688 performance frames. They exercise routed Enter/prefill,
growth, shrink, no-op, Undo/Redo, invocation-relative dot, recording, saved Run,
delayed receipt navigation and reopen, then restore the starting project.
The debug smoke first found the missing CLI scenario allowlist registration.
Its dot comparison then proved that only seven timing-allocation fields differ
between equivalent parameter invocations. The final assertion permits only a
bijection of timing IDs, requiring exact equality of every frozen layout,
placement, phase, window and sound clock. Undo retains a full content comparison
against the correctly awaited preceding state; an earlier wait used a stale
revision and could return before Undo was admitted. No product history or clock
reducer was changed for these replay repairs.

Two final 960×640 release captures were inspected. The fixed-Retime breadcrumb,
duration field, separate clocks, focus and keycaps remain readable, with the
picture at its actual aspect. Both replays retain 14 second-retry frames and
warnings for runs of consecutive retrying frames with distinct causes; no
ignored or repeatedly identical retry fails the contract. Visual capture hits
the intermediate-image allowance while retaining named checkpoints. These
warnings remain in the raw reports; no frame or image limit was raised.

Quiet input-to-state p99 is 1.723583ms across 134 samples. Commit p99/max is
83.303708ms across six samples, and picture completion max is 9.200208ms across
37 samples. None fails or times out. These small scenario samples do not close
general performance acceptance. The executed release app SHA-256 is
`a15255e6819fd946b8ea4efae0b6bb55d4f64f2a3dcda452ca9829d55e2a407b`.

The release export test passes in 14.231s. Four literal layouts verify every one
of 160 pictures and 256,256 presented audio samples, in four complete windows,
at offset zero with no flags, uncovered samples or low-SNR blocks. Minimum
block SNR is 30.469411dB. There are 192 observable neighbor comparisons and 120
explicitly unobservable comparisons between identical neighboring pictures.
Unit-rate fixtures change 3→5 and 3→1 inside output8/input[0,8); affine fixtures
change 3→5 and 3→2 inside output6/input[0,8). Both keep the independent middle
stage unchanged. Every picture expectation independently samples the literal
child at its output center. Unit cases compare four retained nonzero 256-sample
PCM windows within 1e-6; affine phase correctness is covered by the separate
44.1kHz canonical tests, not a rounded root-shift claim.

Store verification uses actually admitted synthetic artifact bytes. It proves
one four-leaf Compound through nested counted Calls, fixed 36-frame extent,
unchanged identities, accepted shortening versus an exact extension request,
Undo/Redo and reopen. It does not claim real inference.

Measured on Mac17,7, Apple M5 Max, 128GiB, macOS 26.5.2(25F84), Rust/Cargo 1.97.1
and locked dependencies. The final 20 Rust source files, baseline 1,933 files,
intermediate source snapshots, source/binary hashes, scripts, failures, full
verification reports and selected captures are retained in
[evidence](../../tools/media-qualification/evidence/2026-10-22-shared-fixed-retime-hold-duration/).
Source archives have per-file hashes and are re-read after collection.
Private artifact sign-in links are excluded.

No requirement or gate is newly complete. DP-06 and DP-07 remain partial.
Physical keys, speech and listening remain owner checks under §29.1. This play,
mixed and selected fixed-Retime durations, wider selectors, processing clocks,
Partition and unsupported sound lifecycles retain their documented limits.
