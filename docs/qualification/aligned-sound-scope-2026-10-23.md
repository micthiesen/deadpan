# Aligned sound-scope candidate evidence, 2026-10-23

The archived candidate passes all 12 focused audio tests after correcting the
permission-family identity. Before that correction, a label-only edit changes
7,581 canonical PCM channel samples and a fragment trim changes 7,570 samples
relative to the shared group-EQ oracle. Both failures have valid numerical
controls. Fresh archives also pass all 1,489 Core tests, all 3,492 selected
ordinary broad tests, 1,292 UI-feature tests and the actual debug-media export
case. Both actual release CLI media cases and the corrected native performance
scenario pass. Renewed strict/full-UI checks and validation of both extracted
release portable archives also pass. Scoped implementation verification is
complete; overall DP/gates stay open and the 50 ms commit target is not established.

## Measured comparison

Both phases use the same tests and supplementary inputs. Their complete
2,037-path Rust manifests differ only at
`crates/deadpan-plan/src/audio_sound_permission_family.rs`. The earlier token
includes complete current BeatSound equality. The correction preserves the
shared permission family across independent label and trim edits, while a
source-recipe replacement still retires its old journal authority.

| Archived phase | Result | Executed audio binary SHA-256 |
| --- | --- | --- |
| `integrated-tenth-before` | 10 pass, 2 fail; 154 filtered out | `683fc431bfe7957d22cd30d0e05d77523b052499a0e120aadeb4afccfc974136` |
| `integrated-eleventh` | 12 pass; 154 filtered out | `37673f39244c8b762efd57c00ae541cc984386d0df3307fb2945a27e4038bccc` |

The label and trim failures span samples 15575 through 19650. The independent
untrimmed public-input/EQ control has zero differences. The complete raw input
matches the independent per-fragment gain calculation in both phases. EQ,
trim and event ablations each change PCM. Fresh read partitions and cold seam
reads agree within each phase. After the token correction, label and trim
comparisons each have zero channel differences; family continuity and public
source-replacement retirement assertions also pass.

The fixture uses the existing actual 8,197-sample WAV provider, a finite
`SelectedPlacement` event from samples `[1024,7432)`, Hard edges, and exact
30,000/1,001 fps and 48 kHz clocks. Public commands create the group, split its
owner, change the label and trim, capture/copy groups and insert time. The
signed G-input API includes independent event gain before owner treatments.
The trim oracle applies one public Equalizer to the complete summed G input;
it does not independently filter each fragment. All PCM comparisons use exact
bits, without an epsilon, alignment correction or listening claim.

## Other focused audio witnesses

The same 12-test archive includes Source, RoomTone Hold and default Silence
controls; unchanged saved tables and historical source admission; refusal of
unsupported processing or placement paths; mixed pre-birth and post-birth
history; incomplete, foreign and forged authority negatives; neutral Split
with EQ and saturation; and Split followed by relocation.

The mixed-history copy retains a nonzero future-input label at sample 30430
without granting extra audible time. The later-born group cannot resurrect a
terminal discarded before its birth. Its independent public identity-Keep route
maps sample 33632 to birth sample 22421 and reports sample 33633 as a gap.
The evidence retains both the original FIR-only output and the separately
masked output. The delivery mask applies after EQ and leaves FIR input intact.

These bounded cases do not establish arbitrary cropped processing, Repeat or
Retime interiors, or unrelated sound lifecycle operations. Earlier
[scope-narrowing research](sound-scope-research-2026-10-23.md) remains the
counterexample context; its constrained passing probes are not implementation
evidence for this candidate.

## Earlier executions and remaining qualification

| Retained phase | Measured result | Scope of claim |
| --- | --- | --- |
| `integrated-eighth-core` | 1,489 pass across 67 binaries; zero skipped | Earlier exact Core source only |
| `integrated-final-core` | 1,489 pass across 67 binaries; zero skipped | Fresh exact Core source after API and test-lint refactors |
| `integrated-twelfth-store` | 3 pass; 63 filtered out | Introduced-group and explicit Split-forest persistence, history, reopen and portable-copy tests |
| `integrated-seventh-plan` | 11 pass; 56 filtered out | Earlier processing-birth geometry/history tests only |

The prior Plan binary is
`3fe101c190d76183f3539ce5e03f5f89fdde635a23ed8126e2cf605c1e74c0d2`.
The Store binary is
`ff227d7cf191f2b34983cc693048725c9a6e08a4469648b5ab41848478a206cc`.
Their receipts identify their own exact source and commands; neither result
claims all Plan or Store tests ran. The retained Core carry-forward receipt
predates the later Core refactor. The fresh `integrated-final-core` execution
supersedes that carry-forward claim for its exact archived source. It reports
125.215 seconds in the nextest summary and 127.9686 seconds in the run receipt.
The Core unit-test executable SHA-256 is
`1c69c724a70b193d5e311d4a9089b280c5f6f355a19774c5f1befffd48392bf9`;
the retained receipt identifies all 67 executed binaries individually.

The `strict-fifth` check and format commands also pass, taking 227.4105 and
7.2381 seconds respectively. Their frozen 2,039-path Rust and six-input manifests
match `integrated-final-core` exactly. These results satisfy renewed Core
qualification for that source. This note marks no requirement or gate complete.

## Broad archive and later UI-only correction

`final-broad` passes all 3,492 selected tests across 109 binaries in 278.422
seconds by the nextest summary, or 281.4366 seconds by the run receipt. Another
739 tests are ignored or unselected. The archive covers the ordinary Plan,
Audio, Analysis, Store and App packages and the exact CLI selection recorded in
the run receipt, with `deadpan-cli/synthetic-worker` enabled. It is not an
all-features UI test run. Its 2,039 Rust paths and six supplementary inputs pass
the recorded source guards, and every archived test executable hash is retained.
The recorded postcheck matches all five archived helpers to the corresponding
root-source executables. Those five archived helper hashes were independently
reread during retention; later source-tree binaries are not substituted.

After that run, the only Rust change is
`crates/deadpan-app/src/preview/harness/sound_group_endpoints/aligned.rs`, from
`6f1fb001e34aeaa510d893b82642ed81cd8412c65a932e769ce88384a40128f8` to
`7ea732fef180dddb205f6cf66edb1723f7b79d29d9233599ec1569d5569afde7`.
The private NativeReplay assertion expects the original owner for the first two
retained clocks and the live full owner for later captures. Frozen phase
manifests independently confirm that the other 2,038 Rust hashes and all six
inputs are unchanged. The frozen `preview.rs` declares this harness behind
`cfg(feature = "ui-harness")`, which the ordinary broad build does not enable.
This attribution preserves the ordinary broad result; it does not establish that
the corrected UI replay passes.

The subsequent `strict-final` check and format commands pass after that correction
in 152.8888 and 6.4768 seconds respectively. Their exact source overlays and input
receipts are retained separately.

## UI-feature archive

The subsequent `final-ui` archive passes 1,292 tests across two binaries, with
11 skipped. It enables `deadpan-app/ui-harness` and runs `all()` from that archive.
Building the archive takes 152.539751 seconds; execution takes 71.820 seconds by
the nextest summary and 72.095371 seconds by the receipt. The 2,039 frozen Rust
hashes and six supplementary inputs match `strict-final` exactly, including the
private aligned-owner replay assertion correction. Both test executable hashes
and five archived helper hashes were independently reread and verified.

The headless test executable SHA-256 is
`54fbe403b50d621184aaa34a86d46bae33a4acc986e174a11ac12a12dd498bbc`;
the app test executable is
`8a8350717d1d86acdc2e88ce7653b3ea1690d5f45407390e1c35abc46e033182`.
This qualifies the archived UI-feature tests. The actual debug-media run is
reported separately below; no native GUI replay or listening success is inferred
from these UI-feature tests.

## Actual debug-media export

The ignored `preview_export` test
`aligned_sound_scope_group_split_move_copy_preserves_canonical_pcm_and_verified_export`
passes from the `debug-media` archive: one test passes, 63 are filtered out.
Execution takes 237.756 seconds by the nextest summary and 237.924668 seconds by
the receipt. The test executable SHA-256 is
`a31090ccce95a5c9c45bd6f4c7ea5b8ca7850602f6f2c1e562c846456c7e8024`.
Its frozen 2,039 Rust paths and six inputs are retained, along with actual test
and four helper executable hashes, build/run receipts and logs.

This fixture admits the actual qualified cfr-bframes Original and uses its
picture moment `[27,31)` with a finite independent Original-audio event from
samples `[48000,52096)`, offset by 37 samples. Event EQ and ancestor Top EQ are
active. A public event-off control changes 3,933 PCM samples and durable Undo
restores the authored state. A five-frame prefix creates the broad retained Top
journal; semantic grouping wraps the complete Source in a new G. Neutral
grouping and the later Source Split preserve every compared authored-PCM,
limited-PCM and limiter-gain bit. New G EQ and saturation affect the numerical
signal. Moving the complete Top, copying G, cold/partition reads, Undo/Redo,
reopen and portable owned-media history all pass their recorded controls.
The unselected Top treatment is not part of the copied G.

The actual emitted MP4 verifies all 19 picture identities and the complete
`[0,30430)` audio window. The measured sample offset is zero, with status
`verified_zero`; all picture/audio flags and the report's failures are empty.
Complete staged PCM arrays, documents, control/history JSON, render report and
emitted-file verification are retained. The movie itself is
[aligned-sound-scope.mp4](../../tools/media-qualification/evidence/2026-10-23-aligned-sound-scope/media/debug-media/exports/aligned-sound-scope.mp4).
The portable package is retained as a deterministic gzip/tar archive with nine
regular members, safe relative names and per-member checksums. Its original
SQLite, owned media and auxiliary bytes are preserved; the archive is a byte
snapshot, not a new execution of package reopen during retention.

This is a debug CLI/media execution with the shared canonical pipeline, not an
independent DSP implementation or a naive PCM-translation oracle. There is no
listening claim. The subsequent release-native failure is recorded below; no
native GUI success is inferred. The independent final Plan/Audio source review found no new
concrete defect, which is review evidence rather than an additional runtime run.

## Failed release-native performance scenario

The `release-native` app build passes in 268.005271 seconds. The actual
`sound-group-endpoints` performance-mode scenario then exits 1 after 13.666357
seconds. Its app SHA-256 is
`1b2bef0b075b64de5aedabe0e1cfb2609aef8965bd03929ce7e70d7b6451ce97`.
The exact executed app and three helper hashes, frozen 2,039 Rust paths and six
inputs are independently verified. The original failed source overlays are
retained without substituting the subsequent harness correction.

At check 104, using the report's zero-based index, the ready helper's settled
assertion expects the committed base revision. Requested, decoded and displayed
identities instead all match the same exact stable Proposed frame 0/source 0.
The report shows no error, no loading and no pending render. Cleanup subsequently
cannot focus “Your edit” because the proposal modal remains open. The complete
JSON and HTML reports preserve every check, timing and finding; build/run logs,
power state, pre-run process inventory and Kestrel source-hash receipt are also
retained. This is a failed runtime result, not a native success.

Root subsequently corrected only the UI harness ready/cleanup behavior using
the existing objects-helper pattern. The subsequent independent CLI release-media
executions outside `ui-harness` and corrected native performance run pass as
recorded below. Its strict/full-UI follow-up also passes as recorded separately.

## Release CLI media

The `release-media` archive builds in 187.298501 seconds and passes both actual
ignored media tests. Each run selects one test and filters out 63. The exact
release test executable SHA-256 is
`c6b486f6af2b6e8c22098eeb35793301623b06b26d3f5933a5f2acf66ac7f4de`.
The actual archived test and four helpers, frozen 2,039 Rust paths and six inputs
were independently checked. Neither run substitutes a newer executable.

| Release fixture | Pictures | Complete samples | Nextest summary / receipt seconds |
| --- | --- | --- | --- |
| Aligned retained scope | 19 | 30,430 | 228.948 / 229.082125 |
| Existing endpoint Split control | 121 | 193,794 | 43.470 / 43.597225 |

Both emitted-file reports pass with `verified_zero` sample offset, no picture
or audio flags, and empty failures. The aligned case repeats the complete staged
authored, limited and limiter-gain PCM controls, independent event-off control,
group/Split/move/copy history and portable-media checks described above. The
endpoint control uses the full qualified Original with one silent frozen prefix
frame, finite Original audio `[40000,56016)` at offset 37, distinct event/owner EQ
and an independent root SoundEvent. Semantic grouping of `[5,11)`, Undo/Redo,
reopen and portable copy preserve complete authored/limited/limiter-gain bits
and the root sound recipe. Its 121 picture identities are checked literally.

The release [aligned movie](../../tools/media-qualification/evidence/2026-10-23-aligned-sound-scope/media/release-media/aligned/exports/aligned-sound-scope.mp4)
and [endpoint movie](../../tools/media-qualification/evidence/2026-10-23-aligned-sound-scope/media/release-media/endpoints/exports/sound-group-endpoints.mp4)
are retained with all staged arrays, control/doc JSON, emitted verification and
render reports. Each portable package has a separate deterministic gzip/tar
archive and safe relative per-member checksums; all nine members per package
were independently reread. This is actual release CLI/export evidence, with no
listening or independent DSP implementation claim.

The retained `release-native-harness-correction-attribution.json` proves the
only changes from the failed native phase are two private UI harness files:
`preview/harness/sound_group_endpoints.rs` and its `aligned.rs` child. Frozen
phase comparison independently confirms the other 2,037 Rust hashes and six
inputs are unchanged. The module remains gated by `ui-harness`. These CLI runs
do not execute the corrected native harness. Its separately measured performance
result follows; no renewed full-UI result is inferred from the CLI tests.

## Corrected native performance run

The `corrected-native` release app builds in 149.074840 seconds and the actual
performance-mode `sound-group-endpoints` scenario exits 0 in 13.819168 seconds.
The exact app SHA-256 is
`f9dc572236d33152948110ce0a93dbf8bd18ebbf207c518c2a1c237ba33f3bea`.
The frozen 2,039 Rust paths, six inputs and all four executed binary hashes are
independently verified. The full HTML and JSON reports, raw timing samples,
power/process/Kestrel receipts and original failed native phase are retained.
All 130 sound-workflow checks and the separate Kestrel check pass.

Both warning findings remain verbatim in
[warnings-verbatim.txt](../../tools/media-qualification/evidence/2026-10-23-aligned-sound-scope/native/corrected-native/warnings-verbatim.txt)
and the complete reports. They record 35 frames settling only on a second retry
and five runs of three or more consecutive retrying frames, each frame with its
own new cause. The separate assertions for no ignored retry and no three-frame
repeat without new input both pass. The warnings are not discarded or counted
as a latency-target pass.

| Native timing | Result | Samples |
| --- | --- | --- |
| Input to state p99 | 1.607166 ms | 233 |
| Input to commit p95 | 88.298625 ms | 7 |

The 50 ms commit target is not established by this run; DP-22 stays partial.
This is a performance-only native run, with no acoustic, physical-display or
readback claim. Its renewed strict/full-UI checks pass as recorded below.

## Final strict/UI and extracted portable checks

After the harness correction, `harness-strict` check and format pass in
152.810192 and 6.519113 seconds. The renewed `harness-ui` archive builds in
66.242773 seconds and passes all 1,292 tests across two binaries, with 11 skipped.
Execution takes 70.029 seconds by the nextest summary and 70.294870 seconds by
the receipt. Both exact frozen 2,039-path Rust and six-input manifests match the
corrected native phase. The actual archived test/helper hashes and all receipts,
logs and source overlays are retained and independently verified.

Root also safely extracts each retained release portable archive and executes
`project validate` and `doctor --project` using the exact archived release CLI,
SHA-256 `36fa88dee464a5a9b90d235db5495b0a423f302bd7102079ab7b186eb820b647`.
Both commands exit 0 for both nine-regular-member packages, report qualified
Originals and Core document 56/Store database 85, and preserve every package path,
size and byte hash before and after execution. The extraction script, complete
receipt, stdout and stderr are retained under
`checks/owner-portable-check/`; the extracted packages are not duplicated in
the evidence. Retention independently matches the tested archive and member
hashes against the already retained portable checksums.

These results complete the scoped implementation checks for this candidate,
while preserving the historical failures and phase-specific source identities.
They do not complete the overall DP requirements, gates or performance work.
DP-22 remains partial, and no acoustic, physical-display or readback claim is
added by these checks.

## Durable evidence and integrity

The [evidence directory](../../tools/media-qualification/evidence/2026-10-23-aligned-sound-scope)
retains complete pre/post result arrays, raw signed input and suppression,
expected and actual PCM bits, ablations, documents, public identities and
delivery masks. Original JSON and logs use deterministic gzip with `mtime=0`;
the summary is not a replacement for those arrays.

Source reconstruction uses immutable Git tree
`5ad2d47c2bab4c5bd842a779615890d9930a33d0` plus exact `.rs.txt` overlay payloads copied from
each archived phase's frozen source directory. Original `.rs` source identities
remain separate from each payload's `archive_path` mapping. Payloads never use a
live Rust suffix, so source walkers and rustfmt do not treat them as modules.
Historical phases keep their original source rather than substituting later
lint-refactored files; final Core and strict-check snapshots are separate phases.
Each phase records its complete Rust path set,
SHA-256 manifest, baseline Git blob comparison, overlay paths and absent
baseline paths. Audio, Core and prior Plan manifests each contain 2,037 paths;
the later Store phase contains 2,038, and final Core/broad/UI/debug-media/release-media/native/strict-check phases each
contain 2,039. Supplementary input bytes and hashes,
build/run/list receipts and logs, executed binary hashes and archive hashes
are retained. Executables and nextest archives themselves remain in scratch
and are not copied into this evidence directory.

All sixteen source snapshots, six supplementary inputs per phase, 253 archived test
executables, twenty-three archived helpers and eight native-phase executables were independently reread and checked
against their receipts.
Compressed originals were decompressed and checked against their original
size and SHA-256. The inventory covers 1,350 payload files and 53,602,717 bytes,
excluding `inventory.json` itself; every inventory entry was independently
reread and verified. Its SHA-256 is
`0c578b4f91199010c5576f48f3b15f39cc392e1d21cac159c39429cb92dddada`.
These checks establish retention integrity, not a new test execution or a
production media-admission receipt.

The first retention package used `.rs` overlay filenames. The strict-third check
found those files as live Rust and failed to resolve their absent sibling
modules. This was evidence packaging interference, not a production compilation
failure. All 238 payloads were renamed to `.rs.txt` with unchanged content hashes,
and the archive-path mappings and inventory were regenerated and independently
verified. Live Rust files were not modified by this repair.
