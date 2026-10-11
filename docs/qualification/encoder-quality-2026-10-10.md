# SDR scene-cut quality, 2026-10-10

Status: implemented and verified through concurrent release and packaged
exports, complete picture comparisons, native sanitizers and the regression
gate in parts. DP-17 remains partial.

This follows the retained [YouTube acquisition failure](youtube-quality-2026-10-09.md).
On the M5 Max, macOS 26.5.2 (25F84), concurrent H.264 hardware encoding reduced
scene-cut frame 3,126 to 30.5441 dB luma PSNR, below the unchanged 32 dB diagnostic
gate. The isolated encode passed. The imported Originals were byte-identical.

## Reproduction and bounded controls

The shared committed-revision picture renderer produced all 3,506 exact I420
inputs from project revision `23aa98c7-712d-4fcc-911b-a9c98a96d7ec`. The raw file
is 10,905,062,400 bytes, SHA-256
`409a88377bf90eaaa99cc278f583dd69b598b78dcb90ae8b7abda9f96c4e820c`.
A native encoder-only reproduction fed these same pictures and silent stereo
PCM through the production C encoder, at 1920x1080, 24 fps, 8 Mbit/s, GOP 12,
without B frames. This removes source decoding, rendering and audio complexity
from the encoder comparison. Every decoded picture was compared to its exact
input, without alignment or resampling.

| Controls | Execution | Frame 3,126 luma PSNR | Lowest luma PSNR, all frames | Largest one-second video rate |
|---|---|---:|---:|---:|
| Original target bitrate | One session | 37.9100 dB | 36.1183 dB | 18.6473 Mbit/s |
| Original target bitrate | Two concurrent sessions | 30.5441 dB | 30.5441 dB | 25.5997 Mbit/s |
| Maximum frame QP 24 | One session | 40.7051 dB | 40.7051 dB | 25.0708 Mbit/s |
| Maximum frame QP 24 | Two concurrent sessions | 40.6278 dB | 40.6278 dB | 25.8432 Mbit/s |
| QP 24, rate ceiling 24 Mbit/s | One session | 40.6278 dB | 40.6278 dB | 23.0725 Mbit/s |
| QP 24, rate ceiling 24 Mbit/s | Two concurrent production exports | 40.6278 dB | 40.6278 dB | 23.0725 Mbit/s |

Both concurrent original-policy encodes reproduce the original failed frame
exactly. Full comparison also finds frame 3,128 below 32 dB. The bounded
candidate's complete single-session comparison passes every frame; its lowest
Cb/Cr PSNR is 44.3819/44.1365 dB. Decoded H.264 block observations show maximum
QP 24 throughout the constrained hardware outputs, versus 48 in the failing
unconstrained output. These are measurements on this runtime, not universal
claims about every hardware encoder or content type.

The constrained reproduction is 158,043,736 bytes versus 141,083,530 bytes for
the failing concurrent baseline, about 12% larger. The original 32 dB quality
gate is unchanged. A short excerpt with black pre-roll did not reproduce the
failure; the complete picture sequence did. Concurrency is a reproducible
trigger on this machine, not an explanation of VideoToolbox's internal policy.

## Implementation and truthful capability admission

`EncodeContract::new_v2` retains v1's pixel-derived starting targets, GOP,
B-frame request, AAC and exact clocks, adding frame-QP 24 and a one-second rate
ceiling of three times the target. The maximum derived rate is 720 Mbit/s;
FFmpeg expresses the requested ceiling as `floor(max_bitrate / 8)` bytes in
one second. Production SDR probes and project encodes both use this constructor
and binding policy 2. HDR and explicit legacy engineering constructors remain
at their previous rate policy.

Native ABI 3 carries both requested controls through configuration, queried
codec observations and strict report validation. Stored ABI-1/2 evidence remains
historical; it cannot enter live admission. ABI 3 retains verification policy 2
and exactly two AAC priming packets. Regression tests reject a spurious third
packet, mismatched priming, omitted controls and altered QP/rate limits.

The first software candidate passed the real scene cut at 45.0347 dB, but a
48-frame 1280x720 changing-grain/edge fixture exposed 19.4211 dB. Its log says
the QP property was ignored. Pinned FFmpeg 8.0.3's
`set_encoder_int_property_or_log` returns success after logging this failure.
An independently retained reproduction of that ignored request again measures
19.4211 dB and maximum decoded block QP 51. Its source, binary, raw reference
and movie hashes are in `quality-ignored-software/report.json`.
The native capability query independently confirms:

- Hardware `com.apple.videotoolbox.videoencoder.ave.avc` exposes both properties.
- Software `com.apple.videotoolbox.videoencoder.h264` lacks `MaxAllowedFrameQP`.

Native admission now queries the supported-property dictionary with the same
hardware/software selection before feeding pixels. An absent property returns
`video_quality_unsupported`, an evidence failure that cannot trigger fallback.
The OS software path is therefore unavailable for current automatic SDR on this
Mac, rather than falsely qualified by an ignored setter. The hardware fixture
passes at 43.2500 dB in two concurrent sessions. The regression accepts software
only if it either passes the same pixel check or explicitly refuses the absent
quality capability. No error text chooses an encoder path.

A counterfactual copy of that regression changes only `new_v2` to the frozen
`new_v1` constructor. Its two hardware outputs measure 19.8681 and 19.8691 dB
and fail the same 36 dB assertion. This uses a scratch test binary and leaves
the shared checkout unchanged. `quality-negative.json` records its exact source,
compiler arguments, linked library hashes and executed binary hash.

## Verification and retained evidence

The regression gate completes in parts. The workspace run executes 5,612 tests:
5,584 pass and 28 fail because ABI-2 movie fixtures and scripted reports no
longer satisfy live ABI-3 admission. Three movies were regenerated with the
real policy-2 encoder, with producer hashes and commands retained. The old
software B-frame movie remains unchanged for a historical full-decoder test
that explicitly proves it cannot enter live admission. All 261 affected and
related follow-ups pass, including every prior failure and that new test.

The final strict workspace/UI Clippy, formatting, 1,099 UI harness tests and
two workspace doctests pass. Native encoder ASan/UBSan checks pass all 63 tests;
only native C adapters and target C dependencies are instrumented, not Rust or
the FFmpeg libraries. Exact flags, executed binary hashes and command results
are in `sanitizer-environment.json`. No live GUI inspection was run.

The first focused run had two stale assertions, corrected without changing
production behavior. One pure protocol unit in the 261-test follow-up reports
a nextest `LEAK` warning; its isolated rerun passes without that warning. This
does not establish the warning's cause. Logs retain both runs. The software
grain test's genuine quality failure is retained and led to the capability
guard above.

The release build and new 741.3 MiB ad-hoc bundle pass. `bundle-verify` checks
a relocated copy with a scrubbed environment: linked libraries, signatures,
local import and Render, bundled AI-runtime Metal smoke, and helper/AI-worker/
notice tamper or deletion negatives all pass. No model pack was installed in
this run. The original bundle and binaries remain separately retained.

## Concurrent public exports

The new release CLI and packaged CLI each render a fresh copy of the original
AV1/Opus project. Both start at 06:19:38 UTC on October 11 and complete Render
about 282 seconds later, with encoding progress advancing together. The packaged
run has a private home and `PATH=/usr/bin:/bin`. Each publishes the captured
revision above with native ABI 3, QP 24 and a 24 Mbit/s rate ceiling.

Both public `verify-export` runs pass all 3,506 frame timestamps, 585 sampled
pictures and 143 zero-offset signal windows. A second full decode compares
every picture against the retained exact I420 input: all 7,012 comparisons
across the two movies pass, with identical minimum Y/Cb/Cr PSNR of
40.6278/44.3819/44.1365 dB. Frame 3,126 is 40.6278 dB in both, and maximum
observed block QP is 24. The original 32 dB threshold is unchanged. Complete
packet scans confirm 24 fps, no reordering, and largest one-second video rates
of 23,072,464 and 23,072,440 bit/s, both below the 24,000,000 ceiling.

| Output | Bytes | SHA-256 |
|---|---:|---|
| Release | 164,306,726 | `90dab71375d52cbe43acebbdc6e37d7983782e0a9959377e35f72956e72a9d57` |
| Packaged | 164,306,721 | `9d74f692fecc793f5c6269f37a66a48d57366c3ff919bc304891e13c0c5c0562` |

`quality-public.json` records the commands, elapsed and wall-clock times,
executed binaries, movies and measurements. The two `quality-public-*`
directories retain publication reports, complete verifier reports and compressed
frame/packet observations. Release CLI SHA-256 is
`2f9544a0f1788d0cbf68cfc727755f495e2b54773822bf347fcdd07407528364`;
packaged CLI is
`45a85153bef6e987b03a36c054aa4789d3b85cc4617202c8a89b13a829a8c3c9`.
`quality-bundle-hashes.json` records the app and helper identities, and
`quality-source-identity.json` binds the build to the changed files over
`fdef263d`. Self-review covered the complete diff, ABI layout, capability
failure classification, probe/project parity and preserved audio/HDR rules.

## Retained artifacts and limits

Scratch evidence is `/tmp/deadpan-youtube-quality-20261009`: `quality-full-matrix.json`,
`quality-bounded-matrix.json`, `quality-all-frames.json`, `quality-packet-rates.json`,
their per-frame/packet data, encoder/decoder source and binary hashes, original
and candidate movies, and `vt-quality-properties.log`. The original failed
software fixture is retained at
`/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpCZoRv7/quality.mp4`.
The raw reference and prior release/bundle binaries remain outside the repository.
Compact reports, experimental source, compressed frame/packet observations and
test logs are retained in the [evidence directory](../../tools/media-qualification/evidence/2026-10-10-encoder-quality/README.md).

This fixes the measured hardware quality failure. It does not establish physical
viewing, all content/raster/runtime combinations, a qualified software quality
fallback, HDR quality-constrained rate control, or the complete release corpus.
