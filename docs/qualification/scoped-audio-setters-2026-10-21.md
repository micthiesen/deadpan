# One-play literal audio setters, 2026-10-21

This increment admits the existing semantic `SetAudio` Trim, Saturation,
Balance and typed Equalizer values under explicit This play at every
enclosing Repeat, without a Visual selection. Native command recording and dot
retain literal parameter values, including explicit stage removal. Each
invocation resolves its current owner and uses the existing scoped
`SetAudioTreatments` transaction. No schema, DSP, timing or encoder behavior
changes are involved.

Existing gain, mute, envelopes, mute ranges, sends and authored stage ordering
survive unless that parameter is explicitly replaced. Isolation maps the owner
and concrete presentation through staged instructions, calls and later pause
or framing operations. A no-op refuses before allocation; stale authority and
late errors leave document, history and registers unchanged. Successful runs
commit and undo once. Broader temporal/ranged audio instructions remain outside
this scoped recording path.

## Failing baseline

Four independently prepared old-code witnesses first proved valid nested 3×2
Play authority, literal values different from the seeded owner, preservation of
unrelated treatment fields and exact ten-node/two-mark isolation pools. The
public raw scoped transaction succeeded and reversed cleanly before each
witness required semantic success. The old semantic allowlist refused all four
setters: exit 100, four failed and 556 skipped. The executed old core test
binary SHA-256 was
`900ef5e37ebdc48012e18327b1ee9961d0389d1b0b31c55e41c1fad0d25d43c0`.
Original source identities, witness source and complete failure output are
retained. This demonstrates the new admission against valid requests rather
than relying on a malformed-input refusal.

## Review and logic checks

Fresh general, scope/atomicity and verification/discoverability reviewers found
no issues. Four literal witnesses and additional cases check first/last and
already owned plays, replace/remove/recreate, composite owners, exact pools,
unrelated treatment preservation, mapped pause/framing continuation and atomic
late failure. Native service coverage uses real managed packages and checks
dot on a different peer, receipts, separate pictures, curves/sends, no-ops,
counted programs and durable Undo/Redo.

The focused core selection passes all 23 tests in 0.034 seconds. The normal
selection passes all 514 tests in 15.065 seconds; the UI-feature selection
passes all 207 in 15.352 seconds. Neither output contains an output-handle leak
diagnostic. Formatting initially caught module order in the native test
registration; reordering the declarations fixed it. The final changed-package
check and strict normal/UI Clippy pass. The generated command reference was
refreshed and its currentness test passes. Test and source identities are
retained with complete logs.

## Independent planned PCM and complete emitted-file verification

The new `scoped-audio-chain` fixture uses the qualified
`cfr-bframes.mp4` Original, SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
The existing Bleep command replaces the sound of Original pictures [12,24)
with an exact 1 kHz tone at -18 dBFS while those twelve pictures keep moving.
A seeded balance of -25 gives unequal stereo. Two inner plays nested within
three outer plays produce exactly 72 pictures and 115,315 presented samples.

One inline scoped Apply, through the real CLI macro entrypoint, submits four
literal instructions to outer play 2 / inner play 2: Trim -6 dB, Saturation
+12 dB, Balance +50 and uniform +3 dB EQ. It retains the register bank, every
pre-existing node, all other occurrences and all picture/duration decisions.
The chosen owner has the independently asserted stage order ClipGain,
Equalizer, Saturation, StereoBalance. Setters and saved intent never supply
the expected numerical PCM values.

The independent steady-state left peak is
`0.5 × tanh(10^(-18/20) × 10^(-6/20) × 10^(3/20) × 10^(12/20))`,
approximately 0.170318686. Right is twice left. Uniform no-cut EQ is a single
central impulse: the complementary band boundaries cancel exactly in the
qualified finite filter. This justifies treating +3 dB EQ as one scalar before
the documented tanh saturation. The unselected occurrences retain left peak
approximately 0.125892541 and right/left ratio 0.75. Distinct noncancelling
trim/EQ gains make omission of either or both observable.

All six 4,096-sample interior planned-PCM windows pass the literal peak and
stereo-ratio oracle. The selected measured left peak is 0.170103729; the other
five are 0.125720009 through 0.125892535. Sample-phase quantization accounts for
the bounded difference from the analytic sine peak. The oracle JSON retains
every occurrence's expected and measured values, window and RMS.

The actual release CLI SHA-256 is
`aa877e6402db34495f53033191edfb51fee9f783875a5d70c9a9bf9f46888c89`;
the executed release test binary is
`d30137f0d1af9ddc3b25de676a59c04b71f86cfdd0ae2c60038e974019668014`.
The named encoded test passes in 40.342 seconds. Canonical Render and
`verify-export --every 1` compare every picture and six contiguous bounded
`--samples` windows covering [0,115315), including both endpoints and every
seam. All 72 pictures pass; all six audio windows are signal, pass without
flags or low-SNR blocks, and report `verified_zero` with measured offset zero.
`summary.nonzero_offsets` is zero. Both the emitted presentation edit and
decoded presented endpoint are exactly 115,315 samples. The file contains
117,760 physical decoded samples and declares 2,048 samples of AAC priming;
the existing MP4 edit metadata presents the authored interval. No PCM alignment,
offset tolerance, packet dropping or verification bypass was added.

Minimum luma/chroma PSNR is 56.455/53.445 dB and maximum thumbnail MAD is
0.050221. Audio compares 241 blocks, with minimum block SNR 11.436 dB and
maximum block level difference 2.412 dB, all under the existing thresholds.
Reports, thresholds, picture provenance and source/output hashes are retained;
the raw media, package and executable binaries are omitted from the repository.
This encoded fixture demonstrates EQ set. Stage removal is covered by logic
and native tests, not claimed as an independently encoded removal fixture.

## Final release replay

The final locked release build, formatting and strict UI Clippy pass.
The executed app SHA-256 is
`7e510af940a625b538127b15d5a7799dacb4a7455f6941ef561d41ffcfd94c19`;
helper identities are retained in `binaries.json`. Both final visual and quiet
performance replays pass all 846 scoped checks and the 21,885,504-case production
Kestrel audit. No ignored or repeatedly discarded layout pass was accepted.

Root inspected three final 960×640 captures: gain recording, mute recording
and the completed literal-setter/removal macro. Scope, separate clocks, controls
and configured keys remain legible. The mute recording retains the preceding
deliberate mismatched-capture refusal; the final setter capture shows the
expected same-state no-edit feedback. Existing inspector-scroll and bounded
layout-retry warnings remain in the reports.

The quiet run has 1,064 completed input-to-state samples: p50 0.480 ms,
p95 0.960 ms, p99 1.388 ms and maximum 1.714 ms. All 323 picture completions
finish without failure or timeout: p50 2.076 ms, p95 15.062 ms,
p99 21.174 ms and maximum 93.165 ms. The visual run's 127.053 ms picture
maximum is retained separately. These are offscreen native measurements on
this Mac, not physical-display or audio-device latency.

The first visual replay failed one existing delayed-pause assertion because it
assumed register `t` was absent. The preceding new setter scenario had saved
that register. The regression now captures the previous macro and asserts it
is preserved after abandonment; the requirement for unchanged saved state is
retained. Strict UI lint passes after that correction.

The second visual replay passed that register-preservation check, then exposed
an existing setup race in `pauses.rs::delayed_source`: copying the real Original
with `Y` was still pending when navigation tried to return to Your edit. The
Hold command correctly refused to overlap that pending action, so the expected
return control was absent. The fixture now explicitly waits after `Y` until
the project is idle, copied content is no longer pending and its Original
moment exists, matching the established recipe setup. This changes test setup
only; production refusal and the later abandonment checks remain required.
Both failed reports, executed binaries, source identities and inspected
failure captures are retained separately from final results. The final replay passes both repaired setup paths.

## Scope of this evidence

Measured on this Mac: Apple M5 Max, Mac17,7, 128 GiB, macOS 26.5.2 (25F84),
Rust/Cargo 1.97.1. Cargo.lock SHA-256 remains
`1e39d18ae4de6c7b898edad39eb7104235fb3ffe3af84521ca94df2b6c25b71b`.
The finite setter admission and authored/encoded result are exercised here;
physical input, physical-display latency and listening are not claimed.
No DP requirement or release gate becomes complete from this slice.

[Retained evidence](../../tools/media-qualification/evidence/2026-10-21-scoped-audio-setters)
contains 89 files (7,312,715 bytes before the inventory itself), with verified
SHA-256 entries in `inventory.json`. It includes original failures, final
reports, source and binary identities, three independent reviews and follow-ups,
the numerical PCM oracle, source/output provenance and five inspected captures.
