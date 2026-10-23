# Compact audio bindings and complete-capture fallback

Implemented, independently reviewed and locally qualified, 2026-10-23.
Five quiet current-format pairs improve the first-pause store-and-refresh
median from 114.587 to 93.633 ms. Picture display is excluded; this does not
establish a percentile or visible-Hold target. DP-24 remains partial.

## Behavior and format

Core 58 / SQLite 87 omits only six exact binding defaults: project-root clock,
empty argument and birth paths, absent resume, zero local resume boundary and
zero phase constant. Nondefaults remain explicit. The standalone reader's
bounded raw preflight counts missing paths as zero entries, retaining its
present-path, duplicate, null, vocabulary, work and byte checks. Tests cover
fixed wire literals, all four clock roots, signed nonzero values, valid nested
placements, malformed paths, patch/inverse restoration and state/document
round trips.

An almost-complete scope falls back after precharge, derived-clock fallback,
duration acquisition and duration-count checks. With at most one omitted node,
ancestor closure or its keyed spacer prevents a smaller projection. Existing
full capture remains authoritative. Randomized all-but-one scopes, empty
roots, unknown aliases above/below the threshold, precharge refusals, sparse
slices and the 10,000-child property cover this behavior.

Unused package schemas 1–86 are refused without mutation; no migration is
added. Standalone binding JSON has no independent header and adopts the current
vocabulary under the owner's development-format authorization. Test-only
historical projections preserve their explicit old-header grammar. Independent
FrozenAudioContext schemas 1–12 and sound-clock formats remain unchanged.

## Verification

| Check | Actual result |
| --- | --- |
| Complete Core | 1,513 tests / 69 binaries; zero skipped |
| Broader selected | 3,499 tests / 109 binaries; 740 ignored or unselected |
| UI feature | 1,292 tests / two binaries; eleven skipped |
| Strict checks | Changed Core/Store Clippy, explicit UI Clippy and formatting pass |
| Debug and release media | Each verifies 15 pictures / 24,024 samples at zero offset |
| Release group/Split/Move/copy | 19 pictures / 30,430 samples at zero offset |
| Portable extraction | Two nine-file packages validate and doctor with all paths/bytes unchanged |
| Large-workload validation | All five edited candidate packages pass full CLI replay unchanged |
| Native replay | 197 workflow checks plus Kestrel pass; two layout warnings retained |

Media controls compare complete authored/limited PCM and limiter-gain bits
where the operation promises equality, including durable history and reopening.
The aligned case separately retains its 24,024-sample neutral group/Split
comparison and 30,430-sample final canonical comparison. It makes no general
Move-as-PCM-translation, independent-DSP or listening claim. Both emitted-file
reports retain zero neighbor observability at repeated pictures.

The native replay is offscreen, without physical display measurement. It retains
51 second-retry frames and seven runs of three or more retrying frames with
distinct causes. Input-to-state p99 is 1.567792 ms over 330 samples.
Input-to-commit descriptive p95 is 90.561250 ms over nine samples; the ordinary
edit 50 ms target remains open.

## Current-format timing

Host: Mac17,7 / Apple M5 Max / 137,438,953,472 bytes RAM, macOS 26.5.2 build
25F84, Rust 1.97.1. Each of five alternating pairs uses fresh packages with
10,000 direct 12-frame Background/Silence Holds, 1920×1080 at 24 fps, and two
12-frame pause insertions. The generator's two setup commits precede timing.
All pairs have no load-above-three flags or path anomalies. Each starts at
10,001 nodes / 120,000 frames and ends at 10,009 nodes / 120,024 frames.

| Work | n | Minimum | Median | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Core 57 / SQLite 86 first | 5 | 113.553 ms | 114.587 ms | 116.431 ms |
| Core 58 / SQLite 87 first | 5 | 92.883 ms | 93.633 ms | 93.827 ms |
| Previous second | 5 | 31.930 ms | 32.896 ms | 33.741 ms |
| Current second | 5 | 28.441 ms | 28.906 ms | 29.641 ms |

The first median improves by 20.954 ms, about 18.3%. This is a short matched
diagnostic, not p95 qualification. The baseline is qualified implementation
`6916ee0d578a11c37e6e66a470eb430b02773764`.
Executed perf SHA-256 values:

- Previous: `d8c0cd0fb54c22f8b697ed7e4c9e4f7944f6ea59b0e55f103c58d2ab61e2e7fd`
- Current: `fa11b331dcd9b2f5645cb011af6f07a54986fa0c0055bcce63bd4af6a4ba3166`

After two pauses, document JSON shrinks from 12,319,659 to 9,136,733 bytes.
The perf observation of main database plus WAL before closing shrinks from
13,592,672 to 7,674,008 bytes. After closing/checkpointing and validation, the
main files are 9,154,560 and 7,589,888 bytes respectively; these are different
observations, not interchangeable storage measures. Perf receipt-assisted
reopening checks head equality. Separate full CLI validation replays every
candidate package without changing its bytes.

Every phase retains exact Rust, six input and executed-binary hashes. Only the
Store expected-value fixture changed after the final Core pass; production
sources stayed unchanged through all later checks. Archived comparisons use
separate Cargo targets. The earlier
[disposable prototype study](first-pause-binding-prototypes-2026-10-23.md)
remains distinct from these current-format results.

## Failures and review

The first build failed on two borrow conflicts in new private tests. After
capturing root IDs before mutation, 1,512/1,513 tests passed; the one failure
exposed a negative fixture without valid resume/reanchor parents. It now starts
from a validated nested state, confirms an untouched round trip, and tests all
six null/oversized paths and three duplicate-placement locations. Only that
integration test changed before all 1,513 passed.

The first broader run passed 3,498/3,499. Its historical range Value expectation
still included explicit default binding fields. The expected current-writer
graph now removes only the six exact default pairs, retaining nondefaults,
the explicit automatic-edge control and untouched historical rows. Only that
Store test changed before the complete passing rerun.

One passing output-handle warning did not reproduce in an isolated same-archive
run or the full rerun. The prior-code witness initially selected default Rust
1.99.1 from its scratch cwd and refused the 1.97.1 libraries. Explicit pinned
Rust with thin LTO gives one expected omission failure and one passing
nondefault control. Its prior binding source matches qualified Core 57; the
comparison library is an early-fallback prototype with an unrelated capture
guard, not the shipping executable.

Independent general/codec reviews found no production defect. Capture review
added a below-threshold unknown-alias case. Follow-up reviews checked both
fixture corrections against valid controls and the immutable historical input.

## Evidence

[Final evidence](../../tools/media-qualification/evidence/2026-10-23-compact-audio-bindings/README.md):
683 payload files / 24,889,683 bytes, independently verified path set, sizes and
SHA-256 values. Inventory SHA-256:
`e75445ac55c307ee5bf020474e5d26602b2898e3a41fad7840f0a58aca75a6ba`.

The historical [91-file Core checkpoint](../../tools/media-qualification/evidence/2026-10-23-compact-bindings-core-checkpoint/README.md)
is retained separately. Final evidence includes emitted movies, portable
packages, canonical numerical controls, all failures, source attribution,
reviews, raw measurements and load reports. Binaries, Cargo targets and nextest
archives remain excluded; their exact identities are retained. Future
large-offscreen/native measurement plans are planning only and prove no picture
latency.
