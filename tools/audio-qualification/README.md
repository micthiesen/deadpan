# Signalsmith Stretch qualification

This developer harness compiles and exercises pinned, real Signalsmith Stretch
DSP. It neither opens audio devices nor adds an application dependency. The
recorded candidate **does not pass all Deadpan adapter targets**. The normal and
sanitizer commands intentionally return 1 while preserving a complete report.
Do not reinterpret those failures as ignored tests or completed Gate A evidence.

Requires Apple Silicon macOS, Python 3.12+, and the Xcode C++ toolchain. Downloads
are source archives for this developer experiment, not an end-user runtime
requirement. The scripts use Python's standard library only.

```sh
python3 -m unittest discover -s tools/audio-qualification -p 'test_*.py' -v
python3 tools/audio-qualification/run.py --output /tmp/deadpan-audio-report.json
python3 tools/audio-qualification/run.py --sanitizers --output /tmp/deadpan-audio-sanitized.json
```

Exit 0 means all declared candidate targets passed; 1 means measured target
failures; 2 means compilation, execution, or measurement failed. A report is
written for failures during the run. `--download-cache DIRECTORY` optionally
reads `stretch.tar.gz` and `linear.tar.gz`; both must match the committed SHA-256
values. Every run extracts to a fresh `/tmp/deadpan-audio-*` directory, retaining
archives, sources, binary, native JSON, and every PCM result, including failures.
The reports identify that path. Files under `/tmp` can disappear; reproduce them
from these pinned inputs instead of treating their paths as permanent artifacts.

`pins.json` fixes both exact source commits and immutable-archive SHA-256 values.
The run verifies those before filtered extraction and records complete MIT
license notices, header hashes, compiler/SDK/host, command arguments, binary hash,
Mach-O load commands, and harness-source hashes. It compiles header-only DSP
directly, with the built-in portable FFT, C++17, `-O2`, and a macOS 15 arm64
deployment target. There is no CMake dependency fetch, external FFT backend,
`-ffast-math`, system installation, or upstream source modification. A deployment
target is not evidence of running on macOS 15.

The 48 kHz stereo fixture is authored in `probe.cpp`: independent steady tones,
chirps, impulses, digital silence, a 4:1 channel-level reversal, and a 20 dB quiet
section. It is 192,192 frames long. The full speed matrix is 1/2, 3/4, 1, 3/2, 2,
each at -7, 0, +7 semitones. Formant compensation is off. All engines start with
seed 1337 and use the default preset. Buffer guards and NaN initialization catch
out-of-bounds or unwritten output; the analyzer checks finite samples and exact
requested stereo lengths. PCM files are interleaved little-endian float32.

Each matrix case measures:

- Whole-buffer `exact()`, fixed 256-output-frame blocks, irregular
  257/509/127/1024 blocks, and a reset/replay with the fixed schedule. Block input
  counts use origin-based nearest-even rational boundaries.
- Output-seek preroll, process calls, and flush separately. Configuration costs
  are measured separately from processing. Whole-buffer timing includes its
  internal preroll and flush.
- Four nonmonotonic local seeks, each starting with 14,400 source frames of
  history and comparing the next 4,800 output frames with whole-buffer playback.
  This uses `exact()` on a local suffix, which internally calls `outputSeek()`;
  it is not a restore of cached internal state or replay from the project origin.
- C++ `new`/`new[]` calls, including aligned forms, on the calling thread during
  library calls. This does not intercept direct `malloc`, OS allocation, other
  threads, locks, page faults, or the audio device callback. Configuration is
  allowed to allocate. After configuration, the configured allocation target is
  asserted for whole-buffer `exact()`, reset, preroll/process/flush in each block
  schedule, and each local-seek render. Reset is explicitly measured by the
  native probe. These results do not establish realtime callback safety.

The independent Python analyzer measures pitch via interpolated positive zero
crossings, RMS level ratios, transient peak position/energy spread, complete
silence, channel leakage, unity identity, and sample differences. Numerical
pitch/dynamics measurements apply to the whole-buffer outputs; every saved
output is checked for length, finite values, and amplitude bound. Chirp frequency,
transient energy width, and embedded-silence tails are diagnostics without an
auditory-acceptance threshold. No listening assessment is implied.

Signal targets in `run.py` were declared before the first measured run: 15 cents pitch,
1 dB channel/dynamic-ratio error, 0.001 unity normalized RMS, 0.0001 partition
and local-seek normalized RMS, 0.0000001 reset normalized RMS, 1,440 samples
transient peak displacement. The zero-allocation target is enforced across all
measured render/reset phases after configuration. The six analyzer tests deliberately
inject wrong pitch, inverted/shifted samples, swapped levels, missing output,
NaNs, and allocations into every measured phase. The allocation test also
changes the target temporarily to verify that assertions use its configured
value, while the production target stays zero.

Two continuous-tone controls were added after the initial failed partition run
to investigate its cause. They retain the same threshold, do not replace the
silence-containing fixture, and do not turn its failures into passes. The report
keeps all failed comparisons. See the [measured qualification report](../../docs/qualification/audio-2026-09-20.md)
for results, primary sources, and integration constraints.
