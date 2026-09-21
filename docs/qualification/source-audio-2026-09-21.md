# Original audio qualification, 2026-09-21

This slice adds an actual persistent audio decoder and host sample cache. It does
not qualify authored import, playback, device output, DSP, resampling, export or
the full required format matrix. All product requirements and gates remain open
or partial. [Implementation](../SOURCE_AUDIO.md) records the contract and limits.

The run used Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and the existing
qualified LGPL FFmpeg 8.0.3 prefix at
`/tmp/deadpan-media-compatible-xyhilms4/prefix`. Native build/runtime checks retain
the exact library pins. The [source manifest](../../tools/media-qualification/evidence/2026-09-21-source-audio/source-manifest.json)
records the base revision and SHA-256 of code, configuration and fixtures.

## Actual sample evidence

Manual-skip mode retains all physical decoded samples. The index separately
records explicit leading skip/discard and the measured final frame duration.
Counts below are sample frames per channel at the original rate.

| Fixture | Rate | Physical decoded | Available measured coverage | Evidence |
| --- | ---: | ---: | --- | --- |
| CFR AAC/MP4 | 48000 | 193536 | `[0, 192192)` | First PTS -1024, explicit skip 1024 and discard; final PTS 191488, 1024 physical samples, duration 704; 320 terminal samples excluded. |
| Offset AAC/MP4 | 48000 | 193536 | `[95072, 288288)` | No leading skip/discard evidence. Preserve the first 1024 samples despite fixture provenance identifying priming. Final duration excludes 320 terminal samples. |
| VFR AAC/MP4 | 48000 | 386048 | `[0, 384384)` | Explicit leading skip 1024; final duration 384 for 1024 physical samples excludes 640 terminal samples. |
| Stereo PCM16/WAV | 48000 | 8197 | `[0, 8197)` | Every f32 equals its authored PCM16 value divided by 32768; final decoded block has 5 samples. |
| Mono PCM16/WAV | 44100 | 44117 | `[0, 44117)` | Every f32 equals the original formula; final decoded block has 3157 samples. No project-rate substitution. |

These intervals describe available measured decode coverage. They are not final
editorial selections. In particular, container start/duration and zero codec
padding claims cannot identify the offset fixture's priming. Unknown leading
samples are not silently removed. Container edit lists remain present in these
AAC fixtures; this is not no-edit-list export qualification.

The independent [C probe](../../tools/media-qualification/evidence/2026-09-21-source-audio/probe/audio_side_probe.c)
and raw/normalized output in its directory retain FFmpeg's observations. It was
built against the pinned prefix with `clang`, `avformat`, `avcodec` and `avutil`.
The integrated native and host tests independently exercise the shipped adapter.

## Verification

- Repository gate: formatting, workspace Clippy with warnings denied, **427 Rust
  tests**, workspace build and headless doctor passed. The existing **83 Python
  tests** also passed. [Commands and raw logs](../../tools/media-qualification/evidence/2026-09-21-source-audio/gate/report.json).
- Native source/media C adapters: **64 tests passed under ASan/UBSan**, including
  owned audio buffers, cancellation, malformed/truncated input, selected-stream
  rejection, resource budgets, exact ranges and shared video/audio input.
  [Sanitizer report](../../tools/media-qualification/evidence/2026-09-21-source-audio/sanitizer/report.json).
  Rust and the separately built FFmpeg libraries were not instrumented.
- Deterministic PCM fixture regeneration matched both committed files and their
  manifest. CI now runs the same `generate_audio_fixtures.py --verify` check.
- Index tests reject malformed schemas/contracts, incompatible formats/layouts,
  nonintegral clocks, missing/contradictory durations, cross-frame skips,
  discontinuities, overflow, and forged derived offsets. Range reads reject
  excluded/out-of-range samples before returning data. Independent sessions
  retain exact audio/video access after the original buffer is destroyed.
- Video stream index 32 is now representable, matching the native allowance of
  one video plus 32 audio streams; index 33 remains rejected.

Native GUI, startup, aesthetics and keyboard tests were not repeated because this
slice adds no controls or lifecycle behavior. The earlier
[native preview observations](source-preview-2026-09-21.md) remain limited to their
tested surface. Listening and device behavior are still untested.

Independent general, timing and native-boundary reviews are complete. They led
to cancellation checks during sample-range scans and index construction, and a
strict header guard before FFmpeg allocation. Post-demux packet checks alone
were insufficient: FFmpeg can allocate a declared packet or table before asking
the bounded I/O callback for its bytes. The guard now validates all admitted
tracks, nested descriptors and semantic table expansion first. Regression tests
cover sparse huge declarations, malformed extents, unsafe metadata, interleaved
tables, bounded WAV packet sizing and one shared opening I/O allowance.
[Review dispositions](../../tools/media-qualification/evidence/2026-09-21-source-audio/review.json)
record the applied findings and final review outcomes.

## Remaining boundaries

The tested container/codec pairs are AAC-LC/MP4 and PCM16 little-endian/WAV.
Matroska is rejected pending safe header and clock/codec qualification. Custom layouts,
other codecs, missing/coarse sample clocks, discontinuous decode positions and
cross-frame skip policies require further work. The host rejects these cases
instead of manufacturing exact timing.

This allocation guard belongs to the new audio adapter. The existing video
decoder's broader container admission still needs equivalent pre-allocation
qualification; these results do not establish a bound for every media path.

PCM cache files are bounded and temporary. Durable index ownership, background
scheduling, large-file performance, cache eviction and cancellation stress remain
open. Import needs an explicit source-to-project audio destination mapping:
normalizing a one-second audio span across a two-second `SourceNode.duration`
would otherwise change its rate. Atomic authored registration/insertion, native
import UI and qualified common A/V origin selection are still required.
