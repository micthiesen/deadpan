# Speed changes after exact pauses, 2026-10-20

This increment adds in-place tape-speed updates for an ordinary Edit Retime
that retains exact pause clocks, under ordinary Sequence ancestors. It keeps
the exact selected child interval and descendant recipes and clocks. DP-07
remains partial. Base `25f40767`, Apple M5 Max, 128 GB, macOS 26.5.2 (25F84),
Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Old-code witnesses

The public-command geometry witness wraps a 12-frame Source into a 24-frame
FollowSpeed stage, between two-frame outer beats. Public `InsertTime` at root
frame 5 inserts three frames and leaves the stage at 27 frames, with exact
input `[0,27/2)`. Updating its output to 15 frames fails on the old code with
`this part of the project is inside a speed change with exact pause timing,
which this view or command cannot use yet`. The retained failed binary has
SHA-256 `8953b78419ecc393d1e1b58180fece0cefd5e7e717616a05c5330b4295fb997e`.
An unchanged raw `SetRetime` passes separately and preserves its clocks and
bindings. The change must retain that admission.

A separate renderer diagnostic starts with a real deterministic 44.1 kHz PCM
provider, a 30000/1001 project clock and a 12-frame Source stretched to 100
frames. Public pause insertion grows that output to 103 frames. The first 256
old samples pass an independently specified phase and step. A test-only
coherent JSON candidate then sets output to one frame while retaining the
exact selected input and original audio lattice. It validates structurally but
the renderer refuses the actual retained transfer of 103 with
`invalid source preparation recipe: source step outside 1/64..64`.
The composed native coordinate step is `45423/4000`, below 64, so checking only
the native rate would miss this failure. Production commands do not use the
diagnostic's JSON construction or bypass the transfer bound.

The diagnostic's first run had a wrong carrier assertion: `[3203,8008)` was
the visible fragment, while its retained full Source carrier is
`[3203,163363)`. Correcting that fixture assertion lets the intended transfer
failure execute. The initial failure and corrected run are retained separately.
Its old 256-sample check establishes entry phase, not complete finite-crop
support.

## Ownership and admission

The shared pure queries `retime_update_input` and `validate_retime_update`
serve the native preview, inspector, semantic planner and command reducer.
Identical raw parameters admit before changed-edit eligibility checks. Changed
families require an integral own output extent, FollowSpeed on the updated
stage and ordinary Sequence ancestors; inner Repeats and processing stages refuse.
Unrelated ordinary stages keep their existing update path.

Existing interior bindings stay byte-identical. Missing interior carriers
capture their pre-edit visible entries. Moved outside owners receive their
own pre-edit entry capture, including existing historical resumes; inert
movement steps may be elided. The reducer updates the recipe and exact extent
coherently, reconciles exact lineage and validates the resulting document.
It does not pass an incoherent covering recipe through ordinary reconciliation
or discard bindings to make an update succeed.

Preflight checks the physical transfer against the actual retained lattice,
including its older local frames per sample. Successive changes cannot reset
this bound to the most recent output duration. The supported inclusive range
is `1/64..64`; carriers with no allocated new output samples consume no
transfer. No schema, DSP, primitive or keybinding changes are needed.

## Empty physical carriers

An additional executed witness finds an overly strict first version of the
new validator. A RoomTone Hold of one input frame followed by 100000 silent
frames is initially played over one output frame. Its full historical carrier
rounds to zero samples. Ordinary growth, followed by a public exact pause,
lets a later output change allocate samples to that Hold while its retained
physical carrier still has none. The renderer returns suppressed silence for
an empty carrier before constructing a transfer, so that unused transfer rate
must not reject the edit.

The retained case changes an exact extent of 100003 to 1000. Its transfer would
be `100003/100000000`, below 1/64, while its complete old carrier is
`[3203,3203)`. The missing-binding control starts with extent 5 and requests 1000;
the actual capture factory creates an empty pre-edit carrier at sample 4805,
and the otherwise-unused transfer is 1/200. Both initial witnesses execute all
geometry and capture checks before failing at the validator's rate refusal.

Admission now checks complete current meaningful support on the old lattice
for both retained and newly captured owners. It deliberately ignores the
narrower visibility of transparent Partitions: a visible fragment can have no
old samples while its full physical carrier is nonempty and still requires
the rate limit. The extra projection is charged to the bounded work budget.

## Verification record

Thirteen focused core/semantic checks, four actual-PCM checks, two durable-store
checks and ten native/inspector checks pass. Formatting, strict Clippy in both
normal and UI-harness configurations, and all 2,807 selected regressions pass
(1,788 skipped by the recorded filter). Independent core/native/store,
audio/export and native/semantic reviews found no remaining concrete issue in
the admitted scope. The retained review record explains dismissed findings.

Both release H.264/AAC exports pass: the shortened result verifies all 19
pictures and the lengthened result all 34, with each complete audio window at
measured offset zero and no verification flags. Maximum block-level differences
are 0.005109 dB and 0.007145 dB; minimum block SNR is 53.043 dB and 32.600 dB,
respectively. The executed CLI SHA-256 is
`80b8cc007b2ec7acff61c23b1222328389a9993b66496374acbb2beba6c6cf3d`;
the release export test is
`a1fcf2b52c358f0753f4c23cf5f02f2cea44a118144b281f78316894b67fc420`.
Those binaries were pinned immediately after export, before the UI-feature
build could replace the CLI executable.

Concurrent security commit `5f9fac98` updates the root lockfile after the
2,807-test run. Its only package-version changes are rustls-webpki 0.103.16
and webpki root certificates/roots 1.0.10; it also resolves tempfile's getrandom
edge to the retained 0.3.4 package. The earlier passing reports and lockfile are
retained separately. Strict UI Clippy and both release exports pass again on
the new lockfile, with identical media measurements. Source manifests separate
the first regression build, the replay focus correction and final dependencies.

The final release visual and performance replays each pass 52 speed-editing
checks plus the Kestrel shortcut audit. The final 960×640 capture was inspected: the exact speed,
Change speed action, separate Original/edit clocks and keyboard hints are
visible. Warnings retain the existing inspector scroll steps at 1280×820,
second layout retries, consecutive retrying frames with distinct new input,
and the intermediate screenshot allowance. Named checkpoints retain their
captures; no ignored-retry or repeated-without-input failure is suppressed.

The performance pass first defers with exit 75 because unrelated Cargo builds
are active; its process snapshot is retained. The final idle-host pass records
input-to-state p99 1.330 ms (64 samples), UI CPU p99 1.330 ms (347 frames) and
input-to-picture-completion p99/max 87.886 ms (15 samples). No timing sample
fails or times out. Thermal state and OS caching remain uncontrolled; this is
scenario evidence, not full performance acceptance. The executed release app
SHA-256 is `4f21175823d62d07892ecea9f5dc392e65ed8e973b009be960c59d338290db9b`.
The root lockfile hash stays
`1e39d18ae4de6c7b898edad39eb7104235fb3ffe3af84521ca94df2b6c25b71b`
through the complete final lint/export/build/replay pass.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-20-clocked-speed-update)
contains the two emitted movies, final inspected capture, source and binary
identities, initial failures and corrected runs, independent review and
commands. Its verified inventory contains 117 files totaling 3,092,054 bytes.

The initial core round passes nine checks and fails two test fixtures that
leave unreachable timing rows after removing or replacing bindings. Fixture
cleanup now preserves only timing records referenced by the remaining state.
The first audio round also catches that cleanup error and the mistaken demand
for an appended inert reanchor. The next round exposes a signed test-counter
error: `saturating_sub` can still produce -1, which was cast to `usize`. The
exhausted-slot count is now clamped to zero before conversion. Full PCM and
suppression comparisons are unchanged. A new external test's use of private
`parent_of` fails compilation and is replaced by the public child iterator.
All failed rounds remain in the record.

The first release replay passes the new pointer update but fails its next
keyboard-prefill assertion. Apply returns focus to Your edit; Enter there
correctly attempts content navigation. The replay omitted Shift-Tab to focus
the inspector first. It now performs that real navigation, asserts Inspector
focus and then tests Enter. An independent follow-up review confirms this
matches the existing nested-pause replay. Production routing is unchanged;
the failed report and its binary/source identities remain retained.

## Limits

This does not admit changed parameters for a fractional own output extent,
inner Repeat or processing family, or a Repeat/Retime ancestor. Attached-sound
transport through a changed processing subtree and broader Section 8 work
remain open. This increment conservatively refuses Source selected-crop-only
empty carriers; the subsequent
[crop-carrier qualification](source-crop-carriers-2026-10-20.md) resolves that
limit and the retained silent-Hold transfer failure. It establishes no new acoustic,
physical-input or latency claim.
