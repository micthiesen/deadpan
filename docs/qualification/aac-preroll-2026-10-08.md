# AAC opening fidelity and fractional edit qualification

This work fixes the pinned AAC encoder's startup distortion of quiet audio.
It does not change the master PCM, sample origin, quality thresholds or event
alignment. Source base: `63d81ca4`; macOS 26.5.2 (25F84), Apple M5 Max arm64,
Rust 1.97.1, pinned FFmpeg 8.0.3 at
`~/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix`.

## Failure and correction

The 10,000-edit fixture originally passed every structural and canonical PCM
check, then failed `verify-export` on its first 480-sample audio block. At the
declared origin, the reference measured -51.682 dBFS and decoded output
-48.116 dBFS, a 3.566 dB increase against the existing 3 dB limit. All 10,100
pictures and the other 33 audio windows passed; offset checks measured zero.
Retained failed run:
`/tmp/deadpan-dp02-fractional-20261008/run-8n4yVn`.

An ordinary pinned FFmpeg transcode reproduced the opening output byte for
byte. Changing AAC coder, TNS, PNS or stereo options did not remove the error.
A diagnostic experiment with one known silent input block before authored
PCM reduced the opening level difference to -0.00366 dB. Those experiments
are retained under the same root in `opening-5`, `aac-options-5` and
`aac-warmup-5`. The experiment alone did not qualify MP4 timing.

The native encoder now supplies exactly 1024 silent samples at PTS -1024,
then supplies every authored sample at its original PTS. The codec adds its
measured 1024-sample delay. Both opening packets remain in the output; one
MP4 media edit starts at 2048 and presents exactly the authored duration.
There is no packet dropping, fitted audio offset, gain compensation or new
AAC-block tolerance. The native ABI and finished-file verification policy
are version 2. Historical stored decisions retain their original evidence;
new live admission requires the current contract.

The actual manual decoder returns -2048 with a 2048-sample skip declaration,
then -1024 with no repeated skip declaration; both are marked discarded.
Ordinary decoding starts at zero. The verifier checks these exact facts,
packet count `ceil(authored/1024)+2`, edit extent, terminal duration and
identical presented PCM from manual and ordinary decoding.

## Verification

`native/deadpan-encode/tests/aac_opening.rs` exercises real encode/decode
with a quiet stereo chirp and a short 800-sample export containing nonzero
sample-zero signal. It inspects independent MP4 packet/edit tables and both
decoder modes. An archived checkout uses a separate Cargo target directory;
the same regression test fails on the earlier encoder's opening SNR and
passes with the correction. Detailed source and executed-binary identities
are retained in `/tmp/deadpan-aac-opening-regression-20261008/results.json`.

The old first encode measures 1.155 dB SNR, below the unchanged 1.5 dB
content-block limit. The corrected first encode measures +0.052572 dB level
and 40.639395 dB SNR; its AAC transcode measures -0.000102 dB and 63.293420 dB.
Both versions execute the identical test source, SHA-256
`434fd660b70eacbf0150c0e78351331b9ee53eee56703764c1e85e8b22487f75`.
The old/new executed binaries have SHA-256
`042fa4f8ad29c520bafb0e19e796c0295bdaedf06979e0a2fc7c04b9021e6b8e` /
`c08d6c7b8e15a0da38c028f6f10536d5e6e807db1b8e9df58ac4a0f7a3048de9`.

The current full encoder suite passes, including the SDR/HDR, cancellation,
limits and deterministic software-output tests. The earlier whole-MP4 SDR
identity changes intentionally because AAC packets and sample tables change;
the test retains the previous identity in its explanation and pins the new
repeatable file.

The repository gate was completed in stages. The workspace run passed 5,438
tests and exposed five outdated synthetic transport claims still declaring
ABI 1. After correcting those fixtures, all ten tests in their integration
binary passed. Final focused validation passed 146 encoded-render library
tests (three existing measurement tests ignored), 23 finished-file integration
tests, retained-decision tests and both default preview/export tests. Strict
workspace and UI Clippy, formatting and both compile-fail doc tests passed.
The opt-in fractional-edit target also passes strict Clippy.
All 1,083 UI-harness tests passed (two skipped). Nextest marked the unrelated
pending-dialog test as leaky once; its focused rerun passed without that mark.
The debug app linker reported its existing oversized unwind-table warning.
The workspace run skipped ten opt-in measurements; this is not a claim that
every opt-in qualification ran. Logs use `/tmp/deadpan-aac-preroll-*`.

## 10,000 fractional-rate edits

Run:

```sh
DEADPAN_FFMPEG_PREFIX="$HOME/Library/Developer/Deadpan/ffmpeg-8.0.3/prefix" \
  cargo test --locked --release -p deadpan-cli \
  --features qualification-fractional-edits --test fractional_edits -- --nocapture
```

`tests/fractional_edits.rs` creates a real 320×180, 30000/1001 fps Original,
inserts one silent Background Hold at frame 50, then extends it 9,999 times
by exactly one frame. Every command commits through the authoritative store.
The independent integer oracle uses the five-frame 48 kHz cycle
`8008*(f/5) + [0,1602,3203,4805,6406][f%5]`. It checks each intermediate
structure, all five fractional residues, original speech resumption and
reopened durable history. PCM comes directly from the independently decoded
Original at the retained resume coordinate, without fitting or interpolation.
One-sample-shift negative comparisons fail at all five residues.

The final release run passed in 928.71 s. Evidence is retained in
`/tmp/deadpan-dp02-fractional-20261008/run-kjgqKE/result.json`, with the actual
project, Original, authored PCM recipe and full verification report beside it.

| Check | Measured result |
| --- | --- |
| Inserted samples | 16,016,000; summing rounded one-frame deltas would wrongly give 16,020,000 |
| Final extent | 10,100 pictures; 16,176,160 samples |
| Shared audition and offline PCM | Every sample checked; maximum absolute error 0 in both |
| Emitted picture timing/content | Every picture passed; terminal PTS 10,110,100 at 1/30000 |
| Emitted audio | All 34 windows passed; both signal windows measured zero offset |
| Audio content blocks | 334 compared; maximum level difference 0.034550 dB; minimum SNR 39.026050 dB |
| Picture content | Minimum luma PSNR 55.265007 dB |
| History | 10,018 new commits, including 18 explicit edge-policy setup commands |
| Bounded work | Six live nodes; maximum document 7,029 bytes; request edit 4,995 bytes; retained scratch 31,519,444 bytes |

Executed CLI SHA-256:
`a83e3140ffc0e48d1bccd7b2ee105a3324b8234d28b491bfb938e27b1a2cefe9`.
Test binary SHA-256:
`d96135e90aefa7ab9968616352e7101cd09bfb129f646d774de1791c840851ec`.
Original SHA-256:
`a1c00a5268ef3adfcba17e7130aa8d51994d0fafc9ec967305a4cec03f818797`.
Published movie SHA-256:
`6d09329c6bab54c058ebf4b557602347ee944103e2e91617543b10cbb974ff67`
(831,399 bytes). Both executed binaries were pinned before the run and
rehashed after it. Later retained-report validation changes were covered by
the focused tests above; they do not change the measured encoder or PCM path.

This establishes the §26.3 repeated fractional-edit timing case for the
specified workload. It is not a 10,000-node performance measurement or the
complete format, VFR and creative-operation matrix. DP-02 remains partial.

## Native reader and packaging

The existing bounded `avfoundation_probe.m` reader and
`native_audio_oracle.inspect_native_audio` passed all three new native fixture
movies (`nonzero`, `software-two`, `marker`) with **zero-sample event
tolerance**. Each reports completed readers, qualified returned PCM timing
and exact marker positions. The source PCM is neither shifted nor fitted.
Raw observations, PCM, checks and commands are retained in
`/tmp/deadpan-aac-preroll-avfoundation-20261008`; the reader binary SHA-256 is
`ff42c3db6827885e5c69089e0acba91f6fb1cdf4132ef8fa0090aa9e2ea94ba7`
and its source SHA-256 is
`8188bc8b3d927b6bc1b4a63ee0f175e9a4dbf9a112af91a283087aa011d76613`.
Fixture bytes and exact producer reports are checked in under
`crates/deadpan-cli/tests/encoded_verification/fixtures`.

A fresh ad hoc signed build at
`/tmp/deadpan-aac-preroll-bundle-20261008/Deadpan.app` passed
`cargo xtask bundle-verify ... --keep`. The relocated copy passed signatures,
load/notices audits, the native smoke test, both doctor entrypoints, bundled
helper probes, project creation and verified Render, and bundled Python/MLX/
LTX/Metal checks. All helper-tamper, missing-helper, AI-worker-tamper and
missing-notices negative checks passed. Its dirty-tree provenance records
the pre-commit source; the final code is unchanged from this build.

| Packaged artifact | SHA-256 |
| --- | --- |
| `deadpan-app` | `a57a0e1c3e2dfc264c217a5e1cffee9468419bc0c2de0ae57d5ef688ef8780ff` |
| `deadpan-cli` | `29710119dc8e4bfd01255fd1b10d84a979236aa3439d60ee5a7472f9bea0b2f3` |
| `build-provenance.json` | `77e739195eee25fceb6d8700d22ae58ab08d5e306488053c086437b55e03cd95` |

These are checks on this Mac. No second-machine, physical listening or display
claim is added. No real-model generation was repeated for this audio change.

## Recipe exports

All 44 release recipe exports passed in 168.88 s with none skipped: 2,300
pictures and 75 audio windows, including all four accepted synthetic Generated
Holds and the 1080p/4K Originals. Every observable audio offset is zero.
Minimum luma PSNR is 38.348940 dB, maximum audio-block level difference
0.684450 dB and minimum audio-block SNR 7.931415 dB. Every file declares
2,048-sample priming. The existing stale-revision and missing-caption negative
comparisons also passed.

The first attempt stopped before exports because its existing release
`deadpan-track` was stale. Rebuilding both release media/landmark workers
resolved the fixture failure; the documented invocation now includes both.
The passing run used:

```sh
cargo build --locked --release -p deadpan-media-worker -p deadpan-track
DEADPAN_REQUIRE_SYNTHETIC_WORKER=1 DEADPAN_PREVIEW_EXPORT_KEEP=1 \
  DEADPAN_PREVIEW_EXPORT_RESULTS=/tmp/deadpan-aac-preroll-preview-export-results.json \
  cargo test --locked --release -p deadpan-cli --features synthetic-worker \
  --test preview_export every_recipe_export_matches_its_committed_preview -- --nocapture
```

The same FFmpeg build prefix above was set. Executed CLI SHA-256:
`d83127143d2bcd3cf4551d20a5ac378c1b254ba8aea2430e21bbf3cc78ece297`;
test binary:
`7ff593c0a33740943a747ab34b7eb99a01e2e545c221a0e80ec023255ff12460`;
result table:
`f4fcbfcacfa2a23ae926a9c7f7454efc7d1a7d3f161a81a41a892207dd6aaf8a`.

The release `preview_export_hdr::hdr_public_render_matches_its_committed_preview`
test also passed in 8.41 s: four public Render cases (PQ, HLG and their grain
variants), 42 pictures each, no measured audio offset and complete matching
PQ content-light metadata. Minimum luma PSNR is 43.553471 dB. Results are in
`/tmp/deadpan-aac-preroll-hdr-results.json`; the log is
`/tmp/deadpan-aac-preroll-hdr.log`.
