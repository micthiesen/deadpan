# Independent SDR file verification, 2026-09-29

The [production verifier](../FINISHED_FILE_VERIFICATION.md) now checks completed
private MP4 candidates in a separate supervised process. Actual byte hashes,
container/sample clocks, continuous picture decode, fresh complete GOP decode and
two AAC presentation modes must agree with the captured contract. Failures retain
the completed candidate for retry. Destination publication and native Render
remain unimplemented.

## Actual media

Measured on Apple M5 Max with 128 GiB RAM, macOS 26.5.2 (25F84), SDK 26.5,
Apple Clang 21.0.0 and pinned Rust 1.97.1/FFmpeg 8.0.3. The FFmpeg build retains
the existing LGPL configuration with networking disabled; this is a development
prefix, not a packaged release runtime.

The fresh Metal run encoded and verified seven committed project ranges. The
unchanged independent reader harness then compared every decoded plane and all
authored PCM against separately prepared direct inputs.

| Case | Pictures | Authored samples | GOPs | Observed B run |
| --- | ---: | ---: | ---: | ---: |
| Structural edit | 108 | 172,973 | 7 | 0 |
| Nonzero one-frame range | 1 | 1,601 | 1 | 0 |
| Explicit software, two B frames requested | 43 | 68,869 | 5 | 1 |
| Odd authored canvas | 1 | 1,602 | 1 | 0 |
| 60 fps markers | 120 | 96,000 | 4 | 0 |
| Accepted Generated provider | 30 | 48,048 | 2 | 0 |
| Encode after cancellation | 1 | 1,602 | 1 | 0 |
| **Total** | **304** | **390,695** | **21** | |

All 912 planes, comprising 116,984,106 component codes, passed the existing
lossy-stage bounds. Manual FFmpeg, ordinary FFmpeg and AVFoundation passed complete
PCM comparisons at fixed timestamps. Both channels' beginning, middle and end
events at samples 100, 48,000 and 95,800 were exact in all three readers: 18
observations, zero sample error. No event-based alignment or AAC-block tolerance
was applied.

The verifier also passed the seven retained files from the previous encoder
checkpoint in 2.66 seconds. The fresh full Metal/encode/reference run took 454.05
seconds; that includes preceding picture and raw-worker qualification, reference
preparation and failure checks. These are individual development runs, not
performance acceptance results.

The hardware path explicitly disables B frames. The software path requests two
and actually emits one. The preceding hardware B-frame failure remains retained
in [encoder qualification](native-encoding-2026-09-29.md); this increment adds no
automatic fallback or hardware policy qualification.

## Regression coverage and retained failures

Native integration tests exercise three real retained MP4s, cancellation after
progress and retry of the same bytes, preflight/runtime failures, eight media
mutations with recomputed SHA-256, and sixteen hostile worker cases. The negative
tests require the intended diagnostics. A separate witness proves the failed-exit
fixture emitted its completion before exiting unsuccessfully. Test-only encoder
replay binds a synthetic background document and does not claim content fidelity;
the fresh Metal run above provides the real project/encoder evidence.

Unit tests cover strict framing and identities, exact report clocks, runtime and
GOP claims, cancellation tokens, hash extent and descriptor identity, packet/NAL
admission, I420 observations, fresh decoder restarts and both AAC modes.

Final checks:

- `cargo test --workspace --locked`: 2,222 passed, zero failed or ignored,
  including the compile-fail documentation test.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo fmt --all -- --check`: passed.
- Selected `deadpan-source` and `deadpan-cli` suites: 241 passed under native C
  ASan/UBSan, zero failures or ignored tests.
- Final normal and instrumented workers each verified all seven fresh MP4s,
  including 304 complete fresh-GOP picture comparisons, in 2.59 and 3.00 seconds
  of measured case time respectively. Each worker exited cleanly.

Instrumentation covers the compiled native C adapters and target C dependencies.
Rust, the C++ DSP adapter and separately built FFmpeg libraries are not
instrumented. Leak detection is disabled. The instrumented reinspection exercises
the source adapters; it does not newly qualify instrumented hardware encoding.

Development failures are retained in the evidence:

- The visible raster was too small a decoder allocation budget for H.264's
  internal padded macroblocks. Storage now rounds to 16x16 while visible
  dimensions remain exact.
- FFmpeg's container codec profile remained unknown without `find_stream_info`.
  Unknown codecpar is now allowed only with both High avcC and actual decoded
  High profile.
- FFmpeg 8.0.3 allocates 2,048 internal AAC samples before returning a 1,024-sample
  LC frame. The allocation allowance now reflects that pinned implementation;
  returned sample counts, timestamps and authored coverage remain exact.
- Review tightened report consistency, retained failed candidates and hash
  budgets, included I420 allocation in its deadline, preserved failed harness
  artifacts, and prevented generic crashes from satisfying negative tests.
- Compile checks caught unsupported digest formatting; Clippy required boxing
  the failure carrying the retained candidate. No lint suppression was added.
- One Clippy invocation was started while final workspace documentation tests
  were still running. That lint process was stopped, its descendants checked,
  and the full lint command rerun after tests completed. The interrupted journal
  is retained separately from the passing final result.

## Retained evidence

The [evidence package](../../tools/media-qualification/evidence/2026-09-29-finished-file-verification/README.md)
contains actual MP4/I420/PCM results, SQLite backup-API project snapshots, complete
process/test journals, strict source inventories, binary hashes, independent
reader results, review notes and every failed qualification attempt. Every
archive member was read back and checked against its size and SHA-256.

The final workspace tests, lint, formatting, sanitized tests and both final
verifier runs share source inventory
`ad271d24c7ba5226183f89e6a4e955014b81d8f7d361e94fd5b4751422874820`.
The fresh encode run preceded small checked-conversion/error-ownership and
qualification-harness changes, recorded by the source comparison. No native
encoder or decoder C implementation changed after that encode run. Final
reinspection binds the final verifier to the retained fresh bytes; current
integration tests cover the host API and candidate ownership.

## Scope

The source guard retains its 16 MiB headers, one million aggregate samples/table
rows and 16 MiB packets. Files outside these bounds fail explicitly, even if the
encoder could produce them. Full-size capacity and sustained performance remain
open.

This verifies SDR structural/decode validity. Arbitrary lossy content fidelity,
automatic platform policy, complete mastering/effects, HDR, durable render jobs
and recovery, verified destination publication, native Render and distribution
acceptance remain required. AVFoundation runs here as a qualification reader;
the production verifier itself uses the pinned software decoder. Every DP
requirement and Gate A through G remains open or partial. No UI behavior changed.
