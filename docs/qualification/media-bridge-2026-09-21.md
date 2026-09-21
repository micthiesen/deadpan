# Native bridge sampling, 2026-09-21

The [media boundary](../MEDIA_CONVERSION.md) now derives the exact interior
bridge frames from the native generated sequence. `canonicalize_bridge` returns
both verified FFV1 masters and the original sampling map from one immutable input
snapshot, with one hard deadline. It does not publish objects, mark candidates
Ready, or accept a provider into the authored document.

The [captured evidence](../../tools/media-qualification/evidence/2026-09-21-bridge-sampling/)
records source, binary, library, input, output, and RGB hashes. The runner freezes
executables before execution. All recorded file hashes and final source hashes
were rechecked when archiving. The base revision is `01249c1`; file hashes identify
the tested changes before their commit.

## Captured-model result

Reference environment: Mac17,7, Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84),
Rust 1.97.1, Clang 21, and the qualified LGPL FFmpeg 8.0.3 prefix from the
[compatible media qualification](media-compatible-2026-09-20.md). The input is
the original 768×320 native sequence from the
[supervised MLX run](model-worker-2026-09-21.md), 25 frames at 24 fps.

The host requested 30 interior frames at 30000/1001 fps. For every output ordinal
`j`, the worker sampled `(j+1)*24/31`, interpolated encoded sRGB RGB8 channels with
integer half-up rounding, encoded FFV1 v3, and independently decoded and compared
every output pixel and timestamp. It retained a separate native master.

| Result | Frames and rate | FFV1 bytes | Last container PTS |
| --- | --- | ---: | ---: |
| Native master | 25 at 24 fps | 4,544,157 | 1000 ms |
| Host-sampled master | 30 at 30000/1001 fps | 5,450,240 | 968 ms |

Both the normal and instrumented runs matched the pre-existing independently
sampled Python result exactly. Packed RGB SHA-256:

- Native: `be4ffce12b048457bcab1779eefe8683f41c242d013be7f488c25416491efe1a`.
- Sampled: `eda362044e57353435b56756be4143419a3bb6ed52bb2f849102b3eff962c71a`.

The normal pair took 4.802 seconds and the instrumented pair 5.757 seconds.
These are single debug-build trials, including conversion and developer output
copying. Power/cache state was not controlled. They are not inference latency or
product performance measurements. Container identities vary per encode; each
returned BLAKE3 reference identifies that run's actual bytes. Rational frame rate,
frame ordinals, and the original map remain authoritative over rounded Matroska
timestamps.

## Verification and review

The required repository gate passed with the explicit FFmpeg prefix: formatting,
Clippy with warnings denied, locked workspace tests and build, and CLI `doctor`.
Results: **293 Rust tests passed, 0 failed, 0 ignored**, and **76 Python tests**
across the audio, model, FFV1-report, and native-harness suites.

The worker's **3 unit, 3 bridge, and 6 existing real-media tests** also passed
under ASan/UBSan. The instrumentation covers the native adapter and target C
dependencies. Rust, the separately built FFmpeg libraries, and leak detection
remain outside that qualification.

The new real-media tests independently derive and hash expected fixture pixels
for 25→30 upsampling, 25→20 downsampling, and 2→1 halfway rounding. They check
native/sampled identity linkage, actual-byte BLAKE3 readback, audio discard,
fractional timestamps, malformed contracts, and a scratch budget sized only for
native frames. Host tests verify one snapshot for a non-seekable reader, pair
consistency, shared deadline expiration, and report rejection. Existing process
cleanup, cancellation, input identity, and protocol-1 conversion tests remain in
the gate.

An initial sanitizer run failed because a test incorrectly required a downsampled
sequence's raw size to be at least the native raw size. The expectation was
corrected to distinguish upsampling and downsampling. The failed report/log are
retained beside the final passing run; there was no sanitizer diagnostic.

Independent reviews covered native arithmetic and bounds, protocol validation,
immutable pairing, shared deadlines, cleanup, and output verification. Review
prompted the developer example to exercise the paired API and retain both masters.
The final reviews reported no actionable implementation findings.

No GUI or native-startup smoke test was run for this media-only increment.
Aesthetics, focus/IME, accessibility, and natural keyboard navigation still
require explicit review as the interactive workspace is implemented.

## Remaining integration

The model protocol must declare native media and provenance, and the host must
bind their verified snapshots to the persisted request's original plan. Complete
provenance, qualified selected-Ready receipts, stale/cancelled acceptance checks,
durable object promotion, explicit undoable acceptance, audition, source joins,
and application rendering remain open. This result does not establish visual
continuity, speech preservation in playback, model-pack quality, or distribution.
No DP requirement or delivery gate is closed by this slice.
