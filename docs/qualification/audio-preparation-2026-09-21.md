# Source audio preparation, 2026-09-21

This milestone implements exact-phase source resampling, a fixed explicit
speaker matrix and verified original PCM access in `deadpan-audio`. It does not
qualify the full audio graph, app playback or export. The base revision is
`cc403cb80e44647abef08e16bdda913830bb2c44`; its separate
[macOS CI run](https://github.com/micthiesen/deadpan/actions/runs/35661500689)
passed before this milestone. The normative specification is unchanged.

## Environment and method

Apple M5 Max, 128 GiB unified memory; macOS 26.5.2 (25F84); Rust 1.97.1
(`8bab26f4f`); Apple Clang 21.0.0; pinned LGPL FFmpeg 8.0.3 from the existing
compatible-media prefix. Release measurements use the workspace release profile
and the production Rust sampler/matrix, without a separate DSP implementation.

The [contract](../AUDIO_PREPARATION.md) documents exact coordinates, finite
support, zero context outside authored trims, supported steps, layouts and
admission failures. No dependency was added beyond the new workspace package.
The example `qualify_source` outputs JSON and hashes with seven sequential
timings per case. Input cloning, channel mixing and convolution are timed;
decode, file I/O, devices, stretch and effects are excluded.

## Measured worker cost and signal checks

Each row produces 256 stereo frames, or about 5.33 ms of 48 kHz audio. Except
the unity-copy row, the source origin has phase 1/3 sample. Timings are seven-run
medians and maxima, not p95 or an app latency guarantee.

| Source samples per output sample | Median ms | Maximum ms |
| --- | ---: | ---: |
| 1/64 | 2.976 | 3.081 |
| 147/160, 44.1 kHz to 48 kHz | 2.980 | 3.005 |
| 1, integer-phase copy | 0.045 | 0.050 |
| 1, fractional phase | 2.849 | 3.017 |
| 2 | 5.610 | 5.818 |
| 8 | 21.987 | 22.060 |
| 64 | 171.603 | 173.550 |

The largest block reads 32,704 source frames in this corpus, below the 32,706
hard bound. The stereo input and mixed staging occupy 261,632 and 523,264 bytes
respectively, plus output and small object overhead. The nine-channel maximum
would require up to 1,177,416 input bytes before stereo staging. The original
decoded disk cache is separate. These are calculated buffer sizes, not a
whole-process peak-memory measurement.

The probe compares actual rendered tones with their analytically shifted
waveforms at 90% of the lower Nyquist frequency. Maximum sample error across
these cases was about 8.04e-7. Sampled downsampling output peaks at the lower
Nyquist were 5.51e-7 or smaller. Seven repeated renders per case had identical
PCM hashes. Tests additionally cover lower passband tones, different phases,
above-Nyquist aliases, upsampling images, impulses and DC. These sampled results
are not a complete swept-frequency, speech/music or listening qualification.

The 2×, 8× and 64× costs show that bounded work is not callback-safe work. Keep
preparation off the device callback and materialize expensive transformations;
worker priority, cancellation under actual load and cache lifecycle still need
application measurements.

## Correctness and admission evidence

The new unit/integration tests cover:

- Bit-equal irregular partitions and isolated seeks, negative source positions,
  fractional phase above 2^53 and translated clocks near both i64 limits.
- Exact integer unity, finite impulse support, independent channels, constant
  gain, full-kernel DC calibration and zero context without trim-edge gain boost.
- Bounded halo reads, missing/malformed/nonfinite/extreme PCM, cancellation,
  unsupported steps, invalid output ranges and checked arithmetic overflow.
- Speaker impulse routing, mono/stereo unity, sparse multichannel ordering,
  unnormalized gain progression and validation of omitted LFE samples.
- Real verified PCM and AAC sessions, terminal samples, arbitrary partitions,
  seven-frame physical read limits, unavailable selected coverage and rejected
  changes to identity, stream, raw observations and retained endpoints.
- Complete index comparison with cancellation between raw and derived chunks.

The committed WAV fixtures decode with unspecified speaker layouts. Initial
adapter tests failed admission, correctly. They now use explicit declarations
from the fixtures' known synthetic recipes, while separate tests retain the
automatic-rejection expectation. Actual AAC has a native stereo layout and
rejects a contradictory speaker override; its unity render is compared with
independently reopened original PCM. That additional test first checked for a
nonzero signal at sample 1000, which is fixture silence. The producer records
impulses at 100, 48,000 and near the end, so the test was corrected to read
`[0,256)`, containing the known first impulse. The failed gate is retained under
`before-aac-window-correction`; the sampler and fixture bytes did not change.
No native parser was changed. The initial qualification example compile also rejected using
`LowerHex` on sha2 0.11's digest array; explicit byte formatting fixed it without
changing hashing or sample expectations.

The exact repository gate passed: formatting, strict workspace Clippy, **671
Rust tests, zero failures and zero ignored**, workspace build and headless
doctor. This includes 23 new matrix, sampler and real-session tests. Raw logs,
measurements, source hashes and independent review are retained in
[the evidence directory](../../tools/audio-qualification/evidence/2026-09-21-preparation/).
The general crate/contract review and separate exact-phase/signal review found
no substantiated actionable defects. The latter reviewer authored the matrix
module but reviewed only the separately authored sampler, tests and probe; the
general reviewer covered the complete crate independently. The high-rate
preparation cost remains an explicit integration limit.
No GUI or startup smoke was repeated because no native UI, focus, keyboard or
lifecycle code changed. Prior [native workspace review](native-workspace-2026-09-21.md)
remains separate evidence.

## Remaining work

Bind historical registered assets and exact plan spans to complete voice
preparation, preserving mixed pitch policies and fractional extents. Implement
authored speaker interpretation, full format/layout corpus, fades, gain and
effects, room tone/tails, oversampled master limiting, long-clip stretch,
prepared-cache lifecycle, device clock/output and preview/export comparison.
Listening, full frequency sweeps and whole-process memory/load measurements
remain open. DP-09 and Gates A through C remain partial.
