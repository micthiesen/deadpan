# Framing ranges in explicit Repeat plays, 2026-10-25

The final-source release build, scoped native replays and complete real-media
comparison pass on this Mac. The distinct original failures and their bounded
follow-ups remain recorded below. Immutable evidence publication and its
independent audit pass, with separate nonrecursive publication proofs.

## Editing contract

Choose This play at every enclosing Repeat, select one visible owner, then
finish `v`, frame motion, `v` before recording or entering framing. The retained
Time selection stays on the root Edit clock. `:zoom` and `:creep` map both
boundaries into the selected owner's exact output extent. This supports Source,
Hold, Sequence, Repeat and Retime owners without expanding plays or decoding.

The bounded plan query first proves the actual non-escalation owner layer at
the captured cursor, then composes compiled parent transforms. Sequence offsets,
stable Repeat iteration identities, existing overrides, explicit gap owners,
Retime slopes and nonzero crops retain their exact rational coordinates. The
owner's full effective output extent supplies normalization. Both integer root
boundaries must lie inside its cropped structural allocation. A frame center
inside a fractional occurrence does not admit a partly owned frame interval.
Provider changes such as cutaways do not change the owner's structural clock;
captured Hold geometry never becomes live ancestry.

For example, a 120-frame Source repeated twice and retimed to 96 root frames
gives the second play allocation `[48,96)`. Root range `[60,84)` becomes exactly
`[1/4,3/4]`, rather than a range based on rounded visible starts. The query
shares one visit/comparison budget across canonical picture descent and parent
ascent. The application and CLI explicitly supply the bounded two-walk budget.

New scoped ranges retain portable Keep or OriginalCenter intent where the
existing static baseline permits it. A fixed Follow admits Keep creep: its
target remains live, both scale bases remain before In, `from=` starts at In,
and both terminal scales hold after Out. The new transition uses OwnerOutput
for the current exact extent; later structural edits retain its normalization
clock. Existing paths, unsupported ranged Follow replacement and sampled
targets retain their explicit refusals. No project schema changes.

Only Zoom and expanded named framing Calls are admitted with scoped Time
selection. Other authored leaves fail atomically, including a later unsupported
leaf in a Call. Native Camera and owner parameters refuse the range; independent
sound controls remain available. Fresh and delayed slice-placement admission
refuse whenever a Repeat/Retime inspector is active, including without a Time
range; Place slice and Replace range controls are disabled. Camera is disabled
with a Time range and retains its existing whole-owner availability otherwise.
Escape clears the range before leaving the inspector. Changes of owner, play or
revision clear it. Scoped selection/navigation remains unrecordable: finish
selection before `q`, then record framing intent. Named and dot replay resolve
the invocation's fresh range and exact play.

The inspector displays the committed Follows, Scale before, Creep, Window and
Framing clock as separate wrapped values. A retained clock also shows its exact
duration and offset. Command entry captures the range, cursor and concrete play
alongside session/revision; later changes cannot retarget the command.

## Verified development runs

Base `cced3e3519e111514e049c8b3e9d96ce3c4acd7d`; scratch
`/tmp/deadpan-scoped-framing-ranges-2026-10-25`. Apple M5 Max, Mac17,7,
128 GiB, 18 logical CPUs, macOS 26.5.2 (25F84), Rust 1.97.1. Locked dependencies
and the qualified FFmpeg 8.0.3 prefix are recorded by each runner. Development
checks use `CARGO_PROFILE_DEV_OPT_LEVEL=2`. Source phases retain the actual
changed Rust inputs and three Cargo/toolchain configuration files; early
Core/Plan-only snapshots are narrower than the later integrated snapshots.

- Source-seventh Core/Plan passes all 2,088 tests. Nine new plan tests exercise
  all owner kinds, exact crops/fractional extents, nested overrides and stale
  paths, owned gaps, Source cutaways, captured Hold geometry and a million-play
  compact lookup under explicit work limits.
- The initial integrated App/CLI run records 2,207 passes, four failures,
  34 configured skips, eighteen slow tests and one passing leak warning. Its
  failures are retained: one new sound-command fixture used the wrong argument
  syntax; two older headless tests expected all scoped ranges to refuse; the
  private disk-full fixture could not keep its volume full. The corrected
  ten-case serial lane passes, including actual qualified scoped ranges, both
  old headless cases and the leak-warning case. A separate five-case serial
  lane passes all scoped inspector/routing tests and real disk-full recovery.
- The host tests remove their linked Original after qualification. Exact
  scoped resolution then uses only retained document/index metadata. They cover
  independent Q32 live/fallback scales, static Keep/OriginalCenter, complete
  timing/audio-plan preservation, fresh named Calls, nested existing overrides,
  atomic refusal and durable Undo/Redo/reopen.
- Source-fourteenth full App records 1,349 passes, two new-fixture failures and
  eleven configured skips. The three corrected real ProjectService tests pass
  serially on source-fifteenth. They verify actual writer admission, scoped
  receipts, all 720 pictures, audio and durable history. A scoped framing edit
  has one transaction step; the fallback terminal retains its exact endpoint.
- Source-seventeenth repeats all 2,088 Core/Plan tests successfully after the
  cache cleanup. Its 33-input source snapshot differs from eighteenth only in
  two App files. Final source-eighteenth strict Core/Plan/App/CLI all-target
  Clippy and scoped rustfmt checks pass. Its optimized development native
  replay passes 296 checks plus Kestrel. It checks preselected recording, backward named replay,
  fresh dot geometry, exact isolation, all 240 providers/layers/captions/audio,
  Undo/Redo, stale entry and full inspector paint. Root inspects the final saved
  window capture. It retains 79 second-retry frames, seven sequences of
  differently caused retrying input frames and the visual capture allowance;
  no ignored retry or repeated no-input retry loop is accepted.

The native fixture uses a 36-frame Retime, an inner three-play Repeat with
four-frame gaps and an outer two-play Repeat with an eight-frame gap, totaling
240 frames. The selected second/second owner occupies root `[164,200)`;
`[170,188)` maps to `[1/6,2/3]`. Its independent oracle preserves the 1.1 prefix,
applies 1.5 at In, ramps linearly to 2.2 at Out and holds through the owner end.
The original fixed document/revision is retained in the actual native report
for subsequent complete-media comparison.

## Final release and media verification

The source-eighteenth release build passes in 161.972 seconds. Its copied app
SHA-256 is `0a12baad871d97031fa7b57f31e475192a0fe363c7f23a1e3ee88596392d902e`;
CLI is `a4919250777aa3968d837bef7a4a4b8d26d7c83079e476cd569d8b2a37fe5107`.
Both are pinned alongside the exact source snapshot and helper identities.
Release scoped replays pass 296 checks plus Kestrel at visual 60 Hz and quiet
120 Hz. The ordinary Original-moment regression passes nineteen plus Kestrel,
including its actual pointer Paste control. Root inspects the saved release
inspector capture. These are offscreen/native-input harness results, not a
physical display refresh or complete packaged-product release qualification.

The real-media run passes in 30.497 seconds against the actual final release
native project. It compares its captured fixed-framing baseline with committed
scoped creep, durable Undo/Redo and a reopened portable copy. A standalone helper
links the exact final Cargo-produced release artifacts; its build passes in
44.199 seconds, with executable SHA-256
`325419f1d39ce72d23ee5667f47cbb9cfad299d2f10539967e19b488b2c5075a`.
The initial attempt fails before child launch because the qualified FFmpeg
library prefix has no command-line executable. The actual independent decoder
is installed FFmpeg 9.0.2, SHA-256
`768609fb3f463ff9272bebac61478515f19e43c5343b0c60dde2e6230c40013e`;
native app/helper libraries remain the qualified 8.0.3 build. The preflight
failure and separate executable/version identities are retained explicitly.

- All 240 pictures verify in each emitted MP4, including 216 live pictures and
  24 captured-gap pictures. The independent exact-pose oracle identifies thirty
  changed canonical pictures over root `[170,200)`; the prefix remains unchanged.
- All 384,384 stereo sample frames of authored and limited PCM, plus each
  limiter gain's f64 bits, match exactly using different preparation partitions
  `[256]` and `[37,251,173]`. Each retained 9,225,216-byte canonical record has
  SHA-256 `e01b60aff7bbe9349b9b16c9ce934f85121a72ec110befce08bd5c369f201b74`.
- Native/control complete finished-file verification passes. Native neighbor
  comparisons are 256 observable/222 unobservable; control is 238/240. Native
  minimum luma PSNR is 46.79791 dB, minimum chroma 56.96131 dB. Audio has twenty
  signal/778 silent blocks and seven uniquely zero-offset segments, with no
  nonzero offset. Whole-file audio SNR is 36.429 dB, minimum block 23.710 dB.
- Separate fixed-reference cross-verification passes all 210 unchanged pictures
  (207 observable/211 unobservable neighbor comparisons). Ten unchanged frames
  have codec residuals; no encoded-byte equality is asserted.
- The portable copy preserves all 37 revisions, twenty Undo-history entries,
  zero Redo entries and one current state. Independent history digest is
  `fd3e54fbf0771637ec10d6c6b20529f83a7ac56fd4b4122f0860904b10b08251`.
  Source persistent paths, bytes, hashes and directories are unchanged before
  and after all CLI operations. SQLite SHM is observed separately as a transient
  sidecar; it is not silently included in the persistence claim.

The media oracle checks the shared canonical renderer and DSP, exact structure
and independent encoded output. It does not independently reimplement the
entire renderer or DSP and does not establish tracking accuracy or listening
quality. Original integrated/full-App invocations retain their failures; the
bounded corrected follow-ups are not represented as a fresh all-green full run.

## Published evidence

The immutable [collection inventory](../../tools/media-qualification/evidence/2026-10-25-scoped-framing-ranges/files.json)
retains 1,101 logical inputs, 604 payload objects and 607 files including
metadata: 18,962,335 stored bytes and 237,179,716 raw/decoded bytes, both below
the 256 MiB cap. It includes eighteen source phases, 84 runtime guards and
all 35 complete qualification receipts. Two independent FFmpeg decoded YUV
rasters are explicit optional omissions; canonical reference rasters and full
PCM records are retained. Final 33 live source inputs match their pins.

Inventory SHA-256 is
`9ca637b223f55acf25b44303265f6c45d9b8da0db1942fe95658671fcd4f778d`;
plan is `338ef288e149e416f0fdfe69aeec4332a44daa9f52579ae3e379121a1add4b57`.
The unchanged approved collector is
`a435a0ee061a871ce9ae107dfc29572bfd7e0c2450955a0ed01b121f31b1f118`.
Actual Root runs dry-run-final13 and collect-final14 pass; collection takes
4.689 seconds, with source and binary pins unchanged.

The independent [final byte audit](../../tools/media-qualification/evidence/2026-10-25-scoped-framing-ranges-proof/audit/published-result-sixteenth.json)
passes through actual Root run byte-audit-sixteenth in 2.661 seconds, including
release/media requirements. Result SHA-256 is
`2281bc2bdbd499aae610bc93e4caa73d8c603f18f58a493307ce80466e1f85fa`;
verifier is `b24c40e2ccc487754d38aa1cf078deaaf6db12ea1d3ed62082a13b0c1c11212d`.
The [proof sidecar](../../tools/media-qualification/evidence/2026-10-25-scoped-framing-ranges-proof/manifest.json)
retains final actual collector/audit receipts, streams, reports and verifier
outside the immutable collection to avoid recursive self-inclusion. Manifest
SHA-256 is `bf3ae1349d684c7e7c1878d1d494d42915ebea5381bd1210c3efec0f4003f1f4`.
Root separately verifies all thirteen listed file hashes/sizes and all seven
JSON documents. An initial malformed sidecar newline is preserved in scratch;
only that nonrecursive manifest is corrected.

Earlier audit failures are retained as evidence-accounting failures: initial
repository-path admission, omission/schema interpretation and receipt scope,
then historical publication-plan associations. Publication/audit receipts have
an explicit separate class from product qualification; the audit's own terminal
receipt is written after its child returns and therefore retained in the
sidecar. None of those failures is relabeled as a product/media failure or a
successful earlier audit. Five passing and three failing native reports stay
distinct. The scope remains final-source release build and scoped replay,
never a complete product release claim.

## Failures and review boundaries

The first plan builds catch a wrong test-module path and Hold field name.
Initial test fixtures also used invalid empty/linked Sources and a cutaway on a
Sequence, which the authoritative validator correctly rejects. Fixtures now use
real stream-bearing Sources and a Source-owned cutaway. The first integrated
builds catch invalid comparison operators for ExactRatio and a missing app
builder path. These are actual failed invocations, not passing results.

The first native run uses `target=keep`, which existing syntax interprets as a
named target. Omitting `target=` supplies Keep. The second run catches a real
remaining ProjectService guard that rejects every Visual selection. The service
now admits only None or Time while Core retains its Zoom/Call allowlist; three
actual service tests cover that boundary. A later replay's injected cursor
change omitted the corresponding picture request, so its strict settled-picture
check correctly failed. The witness now schedules the picture explicitly.
These delayed frame/range/play changes are labeled state-machine injections;
the selection and normal commands use the production native input routes.

General, exact-clock and native/coverage reviews run independently. The initial
reviews do not find the service guard; the actual native replay does. Coverage
review adds actual Follows-label paint and finds direct Place slice/Replace
range callbacks bypassing keyboard admission. Fresh/delayed placement checks
and disabled controls close that path before final native verification. Focused
follow-up reviews find no remaining concrete service or placement bypass.

The Mac initially has about 3 GB free. After Cargo finishes, removing only this
task's rebuildable 53 GB incremental cache recovers about 44 GB; sources,
executables, evidence and user assets remain. The private disk-full recovery
test then passes its serial recheck. The cleanup has its own actual receipt.

No physical display latency, OS IME delivery, VoiceOver speech, listening,
tracking accuracy, other hardware or clean-Mac claim is made. DP-08 remains
Partial. The separate [progress reassessment](../PROGRESS_ASSESSMENT.md) replaces
the inherited 89% with about 83% acceptance coverage, eight Complete/sixteen
Partial and effects five of thirteen. This workflow does not close a complete
ledger criterion; existing creep parameter editing and broader scopes remain.
