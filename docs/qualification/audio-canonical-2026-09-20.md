# Canonical audio scheduling qualification, 2026-09-20

**Gate A remains open.** A worker-side Signalsmith Stretch prototype now passes
the declared consumer-block, replay-seek, prepared-PCM-seek, short-clip, and
synthetic signal checks. It is not connected to the application, an audio device,
the render plan, or the project's artifact/cache lifecycle. No sound was played
or recorded, and no listening or realtime-callback qualification is claimed.

The [raw candidate experiment](audio-2026-09-20.md) and its 82 failed targets
remain unchanged. This follow-up implements and measures a shared scheduling and
state strategy rather than widening those thresholds.

## Implementation and reproducibility

The standalone [`CanonicalStretch`](../../tools/audio-qualification/canonical.hpp)
owns a fixed 256-output-sample DSP schedule. Input boundaries use signed,
origin-based, nearest-even rational arithmetic. Requests from consumers only
drain the prepared block; they never choose the DSP call boundaries. Every
render uses the same seed, parameters, source interpretation, and sample mapping.

The source is zero-extended before and after its authored interval. Initialization
uses the pinned library's output-seek latency compensation. Additional leading
context ends on the canonical block grid. DSP reads use boundaries relative to
the authored origin, including negative context, and output is cropped to the
exact requested sample count. Short clips use this same path. A whole-file
`exact()` call is not substituted for export.

The renderer holds one 256-frame stereo PCM block, or 2,048 bytes, in addition
to the configured library's state. Input access is through a caller-owned source.
The experiment's complete input/output buffers and Python analysis arrays are
fixture storage, not bounded application queues or whole-process memory evidence.
The prototype accepts positive sample counts up to `2^48`, rates from 1/8 to 8,
and pitch from -24 to +24 semitones. Only the matrix below is measured. Twelve
native assertions reject invalid recipes and backward/out-of-range replay seeks.

The pinned DSP is not copy-constructible. Exact cold seeking instead starts a
new renderer and replays the identical schedule from its origin, discarding
preceding output. Its work grows with the seek position. For prepared output,
the harness reads arbitrary slices from a completed PCM file using a bounded
256-frame scratch buffer. These reads can block and belong outside a device
callback. This experiment does not implement cache identity, promotion,
eviction, cancellation, or partially prepared-file availability.

The [native probe](../../tools/audio-qualification/canonical_probe.cpp) reuses
the frozen raw probe's original fixture, guards, and allocation instrumentation
by including it with a renamed, unused entrypoint. It runs its own canonical
experiment. The [runner](../../tools/audio-qualification/canonical_run.py)
hashes both probes, the adapter, analyzer/tests, and original helper/pin files.
Each run verifies and extracts fresh immutable source archives, records notices,
headers, compiler/SDK, Mach-O dependencies, commands, and every PCM hash.

Stretch 1.3.2 is pinned at `57b93f4e9206a089a45387eaa39bdc9f310d3308` and
Linear 0.3.1 at `5668673560146a9cfe38c25315071e3fd68c8317`, both MIT, using
the portable built-in FFT. The upstream [configuration and latency API](https://github.com/Signalsmith-Audio/signalsmith-stretch/blob/57b93f4e9206a089a45387eaa39bdc9f310d3308/README.md)
permits explicit analysis windows and intervals. The library source is unchanged.

## Configuration experiments

Three analysis-window/step configurations use the same 50 cases and unchanged
signal targets: pitch within 15 cents, channel/dynamic ratios within 1 dB,
transient peak within 1,440 samples, and unity normalized RMS error below 0.001.
Consumer and seek PCM must now agree bit-for-bit, a stricter target than the raw
candidate's RMS comparison. No failed alternative is made an expected pass.

| Window / analysis step | Result | Failure retained |
| --- | --- | --- |
| Upstream default 120 / 30 ms | 3,446 / 3,447 | At 2× speed and -7 semitones, the right impulse peak is 3,368 samples late. |
| Shorter 60 / 15 ms | 3,445 / 3,447 | Left-channel pitch errors of +17.587 and +16.201 cents at 0.5× and 0.75×, both -7 semitones. |
| Denser 120 / 15 ms | 3,447 / 3,447 | No declared target fails in this corpus; this costs more DSP work. |

Complete reports: [default-window failure](../../tools/audio-qualification/canonical-default-window-report.json),
[short-window failures](../../tools/audio-qualification/canonical-short-window-report.json),
and [passing normal run](../../tools/audio-qualification/canonical-report.json).
The denser configuration is the adapter and harness default. Selecting it from this small
synthetic corpus is not speech/music listening approval or proof for all rates.

## Measured coverage

The original 192,192-frame stereo fixture contains independent tones/chirps,
separate impulses, silence, reversed channel levels, and a -20 dB quiet section.
Five speeds, 0.5, 0.75, 1, 1.5, and 2, cross -7, 0, and +7 semitones. Thirty
additional cases cross that matrix with 31- and 1,003-sample impulses. Five unity
edge cases have lengths 1, 2, 5,759, 5,760, and 5,761 samples. Short output counts
are rounded once before constructing the exact input/output-length ratio.

Each case renders independently with requests of 256 samples, irregular
1/257/509/127/1,024 samples, and 4,096 samples. Four nonmonotonic seeks include
an unaligned near-end position and the final single sample. Each is reproduced
both by a fresh engine's full replay and by reading the prepared PCM artifact.
The independent Python analyzer compares actual files, not native equality flags.
Before reading PCM, it requires the exact 50-case manifest, three consumers and
four seek windows per case, the requested analysis window, the 256-frame quantum,
and all twelve native rejection checks. Missing, duplicate, malformed, or
misconfigured results are harness errors and cannot become passing reports.

| Check | Passing evidence in the 120 / 15 ms normal run |
| --- | --- |
| Complete consumer output | 100 bit-exact comparisons; 100 equal DSP-call-count checks |
| Replay / prepared-PCM seeks | 200 bit-exact comparisons per strategy |
| Finite, exact-length, bounded PCM | All 558 retained files; 10,365,390 stereo frames |
| Post-configuration C++ allocations | Zero calls and bytes in all 550 measured consumption phases |
| Owned PCM workspace | 256 frames per renderer/cache reader |
| Pitch | -2.402 to +1.684 cents |
| Quiet/loud ratio | -20.0127 to -19.9863 dB |
| Channel ratio | Maximum error 0.1500 dB |
| Transient peak displacement | -8 to +612 samples |
| Unity identity | Normalized RMS 7.45e-8 to 6.37e-7; all 14 short-clip channel peaks at their authored samples |

The maximum measured 90% transient-energy width is 2,401 samples, about 50 ms.
Non-unity short-clip tests establish nonzero, finite, bounded output and exact
consumer/seek consistency; their energy and peak positions remain diagnostics,
not a claim of perceptually acceptable transformed syllables.

Host: Apple M5 Max, Mac17,7, 128 GiB RAM, arm64 macOS 26.5.2 (25F84), Apple
Clang 21.0.0, SDK 26.5. Compilation uses C++17, `-O2`, warnings as errors, and a
macOS 15 arm64 deployment target. That target is not evidence of testing macOS 15.

For the 15 mixed cases, normal-run 256-sample consumption totals 76.420 to
333.272 ms per complete render, or 0.0303 to 0.0430 seconds per output second.
The longest individual consumption call is **15.342 ms**, exceeding a
256-sample device deadline of 5.333 ms. Keep preparation off the callback.
Fresh configuration allocates 896,344 intercepted C++ bytes in 35 calls.
Full-replay seeks take 16.606 to 327.490 ms; immediately rereading the prepared
file takes 0.0083 to 0.0530 ms. Those files were just written and are warm in
the OS cache. These are single-run wall times on one shared workstation, not
p95 latency, cold-disk, sustained scheduling, or application performance claims.
The counter excludes direct malloc, OS allocation, other threads, and locks.

## Verification and remaining work

The [ASan/UBSan run](../../tools/audio-qualification/canonical-report-sanitized.json)
also passes all 3,447 assertions without sanitizer diagnostics. All 558 PCM
hashes match the normal build. Sanitizers instrument the header-only DSP as well
as the adapter and harness; no fast-math or alternate FFT backend is used.

```sh
python3 -m unittest discover -s tools/audio-qualification -p 'test_*.py' -v
python3 tools/audio-qualification/canonical_run.py --window 120-15 --output tools/audio-qualification/canonical-report.json
python3 tools/audio-qualification/canonical_run.py --window 120-15 --sanitizers --output tools/audio-qualification/canonical-report-sanitized.json
python3 tools/audio-qualification/canonical_run.py --window 120-30 --output tools/audio-qualification/canonical-default-window-report.json
python3 tools/audio-qualification/canonical_run.py --window 60-15 --output tools/audio-qualification/canonical-short-window-report.json
```

The recorded runs also pass `--download-cache /tmp/deadpan-audio-discovery`,
reusing verified immutable archives and extracting a fresh tree each time.
The final two commands intentionally return 1 and preserve failed targets;
measurement/build failures return 2. This is distinct from a passing run.

All 20 Python audio-analysis tests pass, including fourteen new canonical tests
that inject consumer phase differences, incorrect seek samples/windows,
consumer-dependent DSP call counts, excess buffering, allocations, silent
output, moved unity impulses, incomplete manifests, and incorrect configurations.
CI discovers these tests alongside the original six. Native DSP experiments
remain explicit developer runs.

The complete repository gate passed with Rust 1.97.1 and locked dependencies:
formatting, warnings-denied workspace Clippy, all 157 Rust tests, workspace build,
and `deadpan-cli doctor`. No native startup/lifecycle code changed, so the
application smoke test was not repeated for this isolated DSP work.

Independent native and evidence reviews checked schedule/seek behavior and the
recorded corpus. Review identified incomplete-output/configuration checks and
an inconsistent constructor default. Those findings were fixed, covered by
regressions, and re-reviewed; all four native reports were regenerated from the
final sources.

The application still needs a Rust/native boundary, plan/asset integration,
asynchronous preparation and bounded callback queues, seek generations, durable
prepared-audio cache management, device/route/suspend recovery, and load/underrun
measurements. Variable rate, tape speed, reverse, resampling, fades, tails,
room tone, gain/mixing, true-peak limiting, speech/music listening, and the
supported hardware/OS matrix remain open. GUI aesthetics, native focus/IME,
accessibility, and keyboard-only editing remain separate obligations for the
actual editor. These experiments do not open the GUI because it would not add
evidence about this isolated DSP path.
