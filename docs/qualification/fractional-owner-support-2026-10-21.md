# Fractional retained audio-owner support, 2026-10-21

A retained processed sample can overlap its following owner's allocation while
starting more than one reference sample before that allocation. The owner query
previously recognized only immediate adjacency. At the qualified NTSC seam,
nonzero Original PCM therefore encountered inactive owner support and the
complete nonlinear master refused preparation. The planner now recognizes the
complete exact overlap. It preserves the original signed lookup coordinate and
rate, including a negative owner-local clock at an allocated boundary.

The production change is one predicate and its comment in
`deadpan-plan/src/audio_owners.rs`. The allocation must remain nonempty, its
provenance must be Retained, and the transferred half-open interval must end
strictly beyond its first allocated point. Only the structural ownership probe
changes. Current-clock inconsistencies, empty support, wholly preceding samples
and genuine exhaustion retain their existing failures or inactive status.
Source admission, DSP, schemas, encoders and native editing gates are unchanged.

## Exact boundary and independent controls

The 30000/1001 project has exactly 8008/5 mix samples per frame. Its Preserve
stage has four output frames and input [1,8). A real lead Hold 3→4 edit moves
the stage's root allocation from [4805,11211) to [6406,12813), retaining its
historical 6406 physical samples. The input body is Source3 + RoomTone2 +
Source4; changing the interior RoomTone Hold to3 keeps the output window fixed.

At consumer sample8236, the retained root lookup is6635. Structural ownership
uses the documented n+1/2 probe, so6635.5 lies after the historical child cut
6635.2. The following Hold owns this sample. The PCM lookup maps to input
13727/4576, giving exact Hold-local coordinate -1/4576 and step35/32032.
The received reference interval starts at64057/20 and advances7/4; it crosses
the first PointCeil Hold point3204 despite beginning more than one point before
it. Clamping the local clock to zero or selecting the preceding Source would
apply the wrong treatment.

The child-gain witness gives the preceding Source -6dB and the following Hold
-12dB trim plus a +3dB envelope beginning at local0. Sample8236 must receive
-12dB, outside that envelope; sample8237 at local1/1144 receives -9dB.
Controls distinguish missing gain, wrong Source ownership, zero clamping and
accidentally applying child gain to the independent event. Cold and irregular
reads retain the same result. The complete suppression mask remains exactly
[12812,12813): the extra current allocation point exceeds the retained physical
history and really contains zero PCM.

Four planner tests cover wide transfers7/4,7/2 and127/4, unchanged work charges,
strict half-open start overlap, zero allocations, genuine exhaustion, current
provenance refusal, odd-label RoundEven tie parity and a valid public
capture/lead-duration history. That pure planner fixture uses ordinary Holds
for blank time; qualified real PCM is tested separately.

## Failing baselines and review

The old production base is `25646268e60830d9fae8be4e3b4d4e586aab39e2`.
Two independent valid PCM regressions each fail one test, exit100. The full
nonlinear master fails with `nonzero Original outside meaningful owner-clock
support`; its executed test SHA-256 is
`a660e05818178706644d3413387a934e13209ad6486367965f469353ca1ee8c2`.
The child-gain witness independently fails in shared EQ with `nonzero Original
outside owner support`; its executed SHA-256 is
`2df57c18ac8177b58b25b40014f9d6cf29dc337a5cdf2307bd07af40efd649e5`.
Exact witness sources, production hashes and complete output are retained.

The first planner compilation failed with E0034, exit101, because sample-grid
methods needed an explicit type. After that test-only correction the old code
ran four tests: two valid positive failures, one strict negative pass and one
invalid fixture setup failure. The latter used a Source with blank video and
no audio, failing validation before its owner query. It is not a third valid
regression. The first implementation had three passes and that same setup
failure. Replacing the invalid blank Source with an equal-duration Hold gives
four valid passes. Production admission was not weakened.

Fresh general, exact-clock/provenance/work and oracle/media reviewers reported
no findings. Root retained the setup/compiler corrections and independently
checked the signed owner clock against structural probe and PCM lookup rules.
Strict lint initially rejected a literal arithmetic assertion as `eq_op`.
The test now translates the actual RoundEven result, and all eight focused PCM
tests pass again. There is no lint suppression.

## Real-media master and export

The fixture creates deterministic 48kHz stereo PCM with nonperiodic431Hz
interpolated hashed knots, asymmetric amplitudes and small DC offsets. ISO MP4
stores source audio as192k AAC with explicit 48k movie timing and delay/padding
edits. All120 H.264 video packets retain their original bytes, PTS, DTS and
durations. Public source admission measures exactly192192 presented audio
samples and video [0,120120) at30kHz. The master oracle uses the actual qualified
decoded Original, rather than assuming the pre-encode WAV remains unchanged.

The first producer used QuickTime MOV, whose H.264 `nclc` color box and `sowt`
PCM track fall outside current admission grammar. The failed admission, atom
inspection, producer manifest and source snapshot are retained. Correcting the
fixture to admitted ISO MP4 changes no production parser. Its AAC delay1024
and discard320 are explicit; the final packet presents704 samples. A separate
pre-run review caught a BLAKE3 content identity compared against SHA-256;
the fixture now checks the actual BLAKE3 identity and retains independent
SHA-256 provenance. Earlier compilation also exposed a path variable shadowing
the picture helper; its E0618 log and source are retained.

The authored nonlinear chain has +6dB saturation on the fixed Preserve output
and +3dB on root, both authored before the lead-duration journal. An independent
44.1kHz event selects native [3100,8005), has +37 mix-sample offset and -18dB
trim, and retains phase2480147/800 with step147/160 over [6443,11782).
Portable project copies retain the same committed history; public commands
remove only that event and the two treatments to obtain current physical PCM.
The independent equation is root tanh(10^(3/20) × processor), with processor
tanh(10^(6/20) × (current physical + event)) in [6406,12813).
It checks all17618 samples before and after the interior Hold edit, including
nonzero physical and mastered sample8236. Controls distinguish stale physical
input, separate-voice saturation and either omitted processing stage.

The release test passes in3.362 seconds. All11 pictures have exact expected
provenance: Background×4, Original13, Background×2, Original13, Original12,
Original13 and Original14. Two contiguous signal windows [0,8809) and
[8809,17618) cover every presented audio sample. Both verify measured offset0,
with no flags, low-SNR blocks or picture anomalies. All38 compared audio blocks
pass, minimum SNR31.051dB, maximum level difference0.118dB. Minimum luma PSNR is
55.579dB. The independent complete master has maximum error3.006e-8 before/after,
and the changed physical operand differs by0.10299, proving it was rebuilt.
The emitted endpoint and edit-list presented-media ticks both equal17618.

Executed release CLI SHA-256:
`ef68f69ad242c16cb4222150b5067512164bad42a5fb525266190ff789c15c8d`.
Executed release test SHA-256:
`f9342f0748fdef2adf5d2ec5e39e624883f4c4f750ac2052499a43a86ddb62c7`.

## Verification and remaining scope

All four focused planner tests pass. The eight focused PCM cases pass again
after the test-only lint correction. The broader selection passes435 planner,
710 audio and89 playback tests, all1234 with zero skips in47.260 seconds.
Final `cargo xtask check --since 25646268 --no-test` passes, including formatting
and strict changed-package Clippy. Its earlier failure remains retained.

The retained evidence directory is
[`2026-10-21-fractional-owner-support`](../../tools/media-qualification/evidence/2026-10-21-fractional-owner-support).
It records actual commands, final source and executed-binary hashes, baseline
failures, review outcomes, complete master equations and emitted-file reports.
Raw media, packages and executables stay in private scratch. Inventory entries
are rehashed after collection.

Verification uses this Mac17,7 M5 Max with128GiB, macOS26.5.2(25F84), Rust1.97.1,
locked dependencies and the qualified FFmpeg8.0.3 prefix. Physical-Mac and
synthetic-worker requirements are explicit. No native UI interaction was
changed or qualified. Ordinary Hold editing inside Retime remains gated in the
app; This-play timing, broader sound-owner edits and complete audio mastering
remain required work. DP-07 and DP-09 remain partial; no requirement or gate is
newly complete.
