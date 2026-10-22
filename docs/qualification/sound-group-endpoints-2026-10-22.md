# Grouping endpoint Splits with attached sounds

Status: implemented and locally qualified for the bounded scope below. No
requirement or delivery gate is newly complete. Retained processing-scope
interiors, root-owned BeatSound transport and broader audio work remain open.

Ordinary `GroupSelection` now uses the existing Split sound transport when its
authoritative preflight requires endpoint Splits. One complete pre-edit lattice
and the command's existing timing identity serve both Original audio and the
independent event journals. Both endpoints retain full processing inputs behind
their delivery crops, prior references and complete sound recipes. Aligned
groups bypass capture before detach, preserving their existing timing tables.

The group parent must stay outside every retained sound processing scope.
The separate ancestry refusal remains. Root-owned BeatSound transport and
removing a Sequence's own sounds remain unsupported. Independent root SoundEvent
coexistence is covered separately. Existing allocation limits remain atomic.
No clock schema, format migration, shortcut or new UI control is introduced.

## Committed-baseline controls

Base: `7ecd222a5cfef8e3cc43f0f9908f4141fe40a913`. All 2,012 Rust paths
and six supplemental inputs match Git blobs. Adding only the public Core
control produces the 2,013-file baseline, built in a separate Cargo target.

Source and Hold witnesses use actual qualified CFR asset metadata with synthetic
authored graphs. Each starts with a direct-root complete owner, independent
finite event at +37 samples, distinct event/owner EQ and a public prefix edit
that establishes an owner-scoped journal. Plain Split and aligned Group controls
pass, including exact inverse bytes, before strict [5,11) grouping fails at
`group endpoint splits cannot yet preserve beat-owned sound clocks`.

Those expected failures take 0.007874s and 0.004467s. Executed SHA-256:
`7653f62b26d4a66e0e47cf8269e01abcdc14a5080908d8d175f5ce3e502167ef`.
All frozen source/input and executed identities remain unchanged.

An independent scratch source copy archives the committed Cargo workspace and
native inputs, with all 1,967 included base Rust files checked against the same
Git-blob manifest. Only the audio test module registration and new control file
are overlaid. Core production code remains the committed baseline, and its
Cargo target is separate from both the active checkout and Core control target.
The complete-PCM plain-Split/aligned-Group control passes in 13.845327s; the
literal same-phase ordinary range-copy control passes in 10.532856s. The desired
endpoint-group and grouped-copy checks then fail specifically at the old guard
in 0.049068s and 0.047819s. Executed SHA-256:
`2100948fc9a61853e49d0ac8316aca6f91c616911a544e70df73c359f5ecdfc5`.
Every source file, non-Rust build input and executed binary remains unchanged.

## Focused checks and retained failures

The first candidate fails compilation because a Store fixture uses ordinary
ordering on `ExactRatio`. The fixture now uses its explicit exact comparison;
production admission is unchanged. No runtime test claim comes from that build.

The second candidate runs nine cases: seven pass and two audio cases fail a
fixture's unconditional source-context-call assertion on cold silent reads.
A silent terminal read can correctly avoid opening media. Only complete reads
now require a historical context call; all exact sample comparisons remain.
The passing grouped-copy and Store cases are retained separately from that fix.

The third frozen candidate passes all 12 focused cases in 14.270691s. Four Core
cases cover Source/Hold endpoint grouping, unchanged aligned root-owned sounds,
root-owned endpoint refusal and the 64-event alias limit. Five PCM cases compare
every sample with 1,021-sample and differing partitions, cold seams and terminal
reads, event/owner EQ, simultaneous Original audio, independent root sound and
same-phase copy. Three Store cases cover complete recipes, exact shared layout,
prior journals, rollback of every durable table, bank preservation, Undo/Redo,
reopen, capture, paste and recopy, plus retained-scope refusal. The audio fixture
uses its existing 8,197-sample WAV; no longer input is invented.

A read-only independent review finds no issue in the bounded transport or its
no-Split and retained-scope behavior. Further integration checks remain pending.
Failed candidates, frozen inputs and reports remain under
`/tmp/deadpan-sound-group-endpoints-2026-10-22` until retained evidence is finalized.

The fourth candidate adds audible RoomTone and silent-Hold PCM cases. Thirteen
of 14 pass; the RoomTone carrier-only control correctly uses current media after
public DeleteBeatSound removes its journal, exposing another overly broad I/O
counter assertion. Historical context is now required only for complete audible
reads that actually retain a sound journal. No PCM, recipe, offset or journal
comparison is removed. The failing case also has a concurrent output-handle
warning. The same retained executable reproduces its original assertion in
2.180131s with both pipes reaching EOF; it remains failed fixture evidence.

The fifth frozen candidate passes all 14 cases in 15.344173s. The RoomTone
control proves the independent event is audible across both seams by comparing
with public DeleteBeatSound, then verifies complete PCM after ordinary Split,
aligned grouping and endpoint grouping. A separate silent-Hold case retains
default independent-sound suppression; no permission policy is weakened.

## Native debug replay

The first native build succeeds with all 2,019 Rust files and six supplemental
inputs frozen. The production-router replay passes 97 workflow checks over
340 frames, plus the local Kestrel compatibility audit. It covers real package
service publication, literal endpoint picture identities, independent Original
cursor retention, quoted group names, Enter/Backspace, Undo/Redo and dot against
a fresh Visual selection. The two named 960×640 captures were inspected:
picture, separate clocks, group scope, breadcrumb and action keys remain visible.

Three warnings remain in the report: the intermediate screenshot allowance,
24 frames settling on a second layout retry, and two runs of consecutive retries
with different input or causes. No ignored-retry paint or repeated-cause failure
occurs. Source, supplemental-input and executed-binary hashes remain unchanged.
Release visual/performance and real export qualification are still pending.

The first export-test build fails on two draft-only `?` operators after the
infallible PCM comparison helper. Both operators were removed; no comparison,
sample, bound, production code or verification tolerance changed. The failed
96.241721s build and its frozen source/input identities remain retained.

## Real-media preflight

The corrected debug test passes in 52.333s. It uses the complete measured
120-picture CFR Original, a silent one-frame prefix, a finite attached voice
from actual Original samples [40000,56016) at +37 samples, distinct event/owner
EQ and a separate root sound. Production CLI semantic grouping resolves [5,11).
All 193,794 stereo authored/limited samples and every limiter-gain bit match the
pre-group oracle after grouping, Undo, Redo and portable owned-media reopen.
Cold seam reads and all 121 literal picture identities also pass.

The actual H.264/AAC MP4 verifies every picture and one complete [0,193794)
audio window, with no flags, uncovered samples or low-SNR blocks and measured
offset zero. Minimum block SNR is 25.388619dB; declared priming is 2,048 samples.
Picture-neighbor discrimination reports 176 observable and 64 unobservable
comparisons explicitly, with minimum margin zero. These do not establish a
unique index from indistinguishable neighboring images; literal plan identities
are checked separately.
The retained test executable has SHA-256
`76682e43689ad795060febbcffb914aca6c254c79f75838f7570c98304655952`.
All frozen sources, six supplemental inputs, test and helper hashes remain
unchanged. This is canonical pre/post equality plus emitted-file verification,
not an independent DSP implementation or listening claim. Release repeats and
broader checks remain pending.

The first broader archive build exposes a test-module integration error:
`bridge_conditioning` and `golden_renders` also include the common recipe
module, but do not define preview/export's root picture helper. The new recipe
now decodes and bounds its own exact Source point, checks the measured 1/30000
time base and literal 1,001-tick picture ordinal, and requires the prefix freeze
or ordinary Source kind explicitly. No production behavior changes. This build
failure and its frozen sources remain recorded; no runtime result is claimed.

## Broader regressions

After that recipe-only correction, the second broader archive builds and all
4,904 selected tests pass in 270.230s (275.437901s driver time), across 173
executed test binaries. The 738 filtered or ignored cases are outside this
selected run; this is not a full workspace or real-model claim. The new Core
unit cases both pass, as do the seven complete-PCM cases and the existing
partial-sound copy/export and fixed-Retime sound regressions. No failure,
timeout or output-handle warning appears. All 2,019 frozen Rust paths, six
supplemental inputs, test binaries and helpers retain their hashes.

The first strict check rejects three constant-equality assertions in the export
fixture (`clippy::eq_op`). Its independent integer numerator now comes from the
measured full Original duration plus the inserted frame, retaining the literal
quotient/remainder and 193,794-sample expectation. This only strengthens the test
oracle; no production Rust changes after the passing broader run. The failed
197.070240s lint result remains retained. Final lint and release media execution
will use the corrected test source.

The corrected changed-package check passes in 152.977298s, UI-feature strict
Clippy passes in 55.696253s, and formatting passes in 6.530070s. All 2,019 Rust
files and six supplemental inputs retain their `lint-second` hashes throughout.
Complete UI-feature tests and release qualification are still pending.

All 1,292 UI-feature cases then pass in 70.799s (71.472778s driver time).
Eleven existing opt-in cases remain skipped; the run is not real-model,
physical-input or listening evidence. Source, supplemental inputs, test and
helper hashes remain unchanged. Release qualification is still in progress.

## Release native replay

The release app builds in 266.393262s. Visual replay passes 97 checks over
355 frames in 15.227452s; quiet performance replay passes 82 checks over 486
frames in 14.321434s. Each also passes the local Kestrel audit. Performance
omits the visual-only pixel checks. Both named 960×640 release captures were
inspected and are byte-identical to the corresponding debug captures.

The quiet run records input-to-state p99 1.419709ms over 161 samples,
input-to-picture p99 87.844084ms over 49 samples and input-to-commit p99
78.901292ms over just three samples. The sparse commit measurements do not
establish broad edit latency. The process inventory shows no competing
compiler/app process before that run; power state is retained.

Visual/performance retain three/two report warnings respectively, covering
the screenshot allowance and layout retries, with no failed check or timeout.
Full reports preserve retry causes. All 2,019 frozen
Rust files, six supplemental inputs and executed native/helper hashes remain
unchanged. Release media verification is still pending.

## Final release media and owner fixture

The final release archive builds in 180.831756s and its export test passes in
44.491s (44.784057s driver time), exercising the corrected self-contained picture
oracle and runtime-derived integer sample calculation. All 193,794 authored
and limited stereo samples and limiter-gain bits match through grouping,
Undo/Redo and portable reopen. The emitted MP4 passes all 121 pictures and the
complete [0,193794) audio window at measured offset zero. Minimum block SNR is
25.388619dB, with seven blocks, no flags, uncovered samples or low-SNR blocks.
Neighbor observability remains 176 observable/64 unobservable comparisons;
those limits are not concealed by the passing result.

The 42,563-byte movie has SHA-256
`4dd472fff6ce0c9c5d33ca315c11abe59e54202e6c4e317bfc4d9e6fffeb7765`.
The safely extracted nine-file owned-media package passes full project
validation and doctor, retaining every path and byte after both commands.
Its 95,646-byte archive has SHA-256
`fd6058082c156ebd3fac6002369823295fdbe7a17d1e895be89c7cb4469de5c9`.
All final frozen sources, supplemental inputs and executed binaries retain
their hashes. Canonical pre/post equality and emitted-file verification do not
establish independent DSP correctness, listening or broader physical behavior.

Retained evidence lives in
`tools/media-qualification/evidence/2026-10-22-sound-group-endpoints`, including
failed candidates, passing baseline controls, exact frozen payloads and path
manifests, executed identities, full PCM oracle, MP4, owner archive, reports,
independent reviews and four inspected captures. The content-addressed archive
also retains the old-audio workspace's non-Rust build inputs. Inventory and
payload hashes are verified during collection. The final source-to-Git audit
is recorded separately after the scoped code commit.

The source audit binds code commit
`6a7eed92e239add1d3a3b6522d8810826c6b52ac` to all 2,019 final Rust paths and
six supplemental inputs. Each recorded SHA-256 matches its exact Git blob;
the tracked Rust path set is complete. No runtime source changes follow the
passing final strict/UI/release runs. Collection verifies 2,575 unique frozen
payloads; the final inventory includes this audit and its code-commit identity.
