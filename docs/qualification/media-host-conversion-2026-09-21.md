# Rust host generated-video conversion, 2026-09-21

The [implemented converter](../MEDIA_CONVERSION.md) preserves the captured model
worker's sampled and native RGB sequences through FFV1 v3/Matroska. Both normal
and ASan/UBSan adapter runs independently decode and compare every pixel and
timestamp. This qualifies the narrow conversion boundary, not complete candidate
admission, application playback, model selection, or distribution.

Evidence is retained in
[the captured reports](../../tools/media-qualification/evidence/2026-09-21-host-conversion/).
They record exact source, input, binary, native-library, output, and RGB hashes,
commands, hardware, compiler, and timing. The captured runner copies executables
before use so a concurrent Cargo rebuild cannot change the measured binary.

## Actual media results

Reference machine: Mac17,7, M5 Max, 128 GiB, macOS 26.5.2 (25F84), Clang 21,
Rust 1.97.1. The helper links the isolated signed-source LGPL FFmpeg 8.0.3 build
from [the compatible media qualification](media-compatible-2026-09-20.md).
The tested library hashes and actual `otool -L` output are in each report.

| Captured sequence | Exact source | FFV1 bytes | Normal seconds | Instrumented seconds |
| --- | --- | ---: | ---: | ---: |
| Sampled Hold | 768×320, 30 frames, 30000/1001 fps | 5,450,240 | 2.442 | 2.704 |
| Native generation | 768×320, 25 frames, 24 fps | 4,544,157 | 2.190 | 2.309 |

These are single debug-build trials, run concurrently for the two build variants.
They are not release performance targets or inference latency measurements.
Both outputs contain one silent FFV1 v3 BGR0 stream with slice CRC, full RGB
range, sRGB transfer, and BT.709 primaries. Container PTS are 0…968 ms and
0…1000 ms respectively. Exact rational timing remains separate from Matroska's
millisecond clock.

Packed RGB SHA-256, excluding the old fixture header and timestamp records:

- Sampled: `eda362044e57353435b56756be4143419a3bb6ed52bb2f849102b3eff962c71a`.
- Native: `be4ffce12b048457bcab1779eefe8683f41c242d013be7f488c25416491efe1a`.

The host's decoded hashes match independently retained RGB fixtures from the
[earlier FFV1 qualification](ffv1-2026-09-21.md). All reported source, frozen
binary, output-file, and sanitizer-log hashes were rechecked when archiving.
The two encodes can have different container bytes because Matroska assigns
identities per encode; each returned BLAKE3 reference identifies its actual file.

## Automated verification and review

The exact repository gate passed with the explicit FFmpeg prefix:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo run -p deadpan-cli -- doctor
```

Result: **285 Rust tests passed, 0 failed, 0 ignored**. The audio, model,
FFV1-report, and native-harness Python suites passed **76 tests** combined.
The sanitizer build passed the worker's two unit tests and six real-media
integration tests. Its C adapter and target C dependencies are instrumented;
Rust, FFmpeg libraries, and leak detection are not qualified by that run.

The new tests cover independent RGB hashes and actual-byte BLAKE3 verification,
one-frame and fractional-rate sequences, generated audio removal, corrupt
pictures, missing tags, wrong count/dimensions/rate, byte budgets, declared
identity/length mismatch, bounded malformed replies, cancellation, hard deadlines,
and process-group cleanup. Most checks run without a GUI or model inference.

Independent agents reviewed the safe host and native implementation. Applied
review fixes include bounded probe options, denied secondary I/O, fixed stream
option-array ownership, codec padding bounds, final cancellation checking, and
recording sanitizer-harness timeout failures. Same-version replacement of the
trusted developer prefix remains an installation/packaging concern; compatibility
checks are not claimed to authenticate mutable local binaries.

## Failures retained and limits

- A tiny fixture exposed H.264 coded padding beyond visible dimensions. Decoder
  allocation now permits a bounded aligned envelope while every visible frame
  must still match the exact requested dimensions.
- The first audio fixture accidentally had no audio stream. It was regenerated
  with a real AAC stream, independently inspected, and now proves discard count 1.
- A transitional test assumed a fixed whole-file FFV1 hash across encodes.
  The [failed sanitizer report](../../tools/media-qualification/evidence/2026-09-21-host-conversion/failed-golden-expectation.json)
  and log are retained. The corrected test hashes the returned bytes independently;
  fixed RGB expectations remain unchanged.
- Initial sanitizer linking with only `-fsanitize` failed on unresolved runtime
  symbols. Applying the runtime link flag to host proc-macro libraries then failed
  because interceptors loaded too late. The committed harness explicitly selects
  the target and links Clang's runtime there, following its
  [runtime linkage requirements](https://clang.llvm.org/docs/AddressSanitizer.html).

No GUI/lifecycle code changed, so native startup, visual, keyboard, IME, and
accessibility testing were not repeated. No model inference was rerun. A complete
native/sampled/provenance bundle, selected-Ready admission, explicit durable
acceptance, app preview, export, aggregate memory-pressure scheduling, signed
bundling, and clean-machine checks remain open. No DP requirement or delivery
gate is marked complete by this slice.
