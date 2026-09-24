# Limited sequence audition, 2026-09-24

Deadpan now has a shared bounded limiter for the current edge-faded stereo bus.
Sequence audition and `inspect-audio --limited` consume `LimitedAudio`; monitoring
gain follows its canonical samples. Independent checks pass on 25 native outputs,
and real-source measurements show useful sequential preparation after bus caching.
This qualifies a limited audition path, not a completed master or DP-09.
The [mastering contract](../AUDIO_MASTERING.md) retains the full effect order,
finite measurement boundary and remaining product scope.

The [retained evidence](../../tools/audio-limiter-qualification/evidence/2026-09-24-native/README.md)
includes the failed candidates, frozen native build, independent auditor,
actual PCM/gains, source indexes, pipeline harnesses, timings and hashes.

## Implemented boundary

`LimitedTile` pins
`deadpan-master-kaiser64-bs4-trigger17-q32-four-pass-fft1280-v1`: published BS4,
sample magnitudes and fifteen fractional Kaiser phases with radius 64. The native
detector uses fixed absolute 1024-frame tiles and 1280-point transforms. Four
bounded correction passes apply linked stereo gain, targeting −1.7 dB only when
the measured peak exceeds −1 dB. Integer Q32 attack/release recurrences feed one
cumulative f64 gain applied to the original f32 bus. Final direct finite checks
inspect the rounded output, including real project-edge reconstruction support.
Failure returns no tile; there is no normalization, clipper or alternate solver.

Context is bounded to 131072 stereo frames and returned output to 8192. A full
interior tile needs 66688 bus frames. The four-tile output cache and twelve exact
bus ranges retain complete source dependencies and revalidate every reuse.
One deadline covers preparation, verification and publication. Source/media work,
DSP and allocations stay off the device callback. Validated silence retains its
signed-zero bytes; an unchanged correction skips identical remaining passes.

Focused regressions cover finite context, actual project edges, wide sample
coordinates, exact silence, tiny fragments, cancellation, deadlines, atomic
failure, source changes within a request and hidden Preserve dependencies.
Cache review added explicit coverage that reuse never expands a required range
into unavailable or unsupported neighboring media. Six limited-reader integration
tests passed after that change. Existing raw and edge-faded APIs keep their
separate meanings.

## Independent final PCM checks

The frozen native build rendered 25 inputs through 39 canonical tiles. All pass
both independent finite probes: exact-integer BS4 with sample floor and complete
trailing support, and a separately pinned radius-96 Blackman-Harris reconstruction
with 32 phase positions. The worst results, both on the unchanged silent-gap
fixture, are −1.292528 dB and guarded −1.099019 dB respectively. The latter's peak
dot product also received an 80-digit check using the stored coefficients and PCM.

Thirteen fresh shuffled preparations match canonical PCM and f64 gains exactly.
Every original zero retains its bits, no active frame disappears, and shared gain
recreates both output channels. Fourteen whole inputs remain byte-identical,
including quiet controls, ordinary 0.5/0.8 tones, the long 0.8 tone, one/two-sample
fragments, permitted tail and both retained earlier-processed failure inputs.
Ordinary 0.8 DC is attenuated: its finite endpoint reconstruction requires limiting.
These observations do not establish all subnormal behavior or audible transparency.

All 25 native PCM and gain files happen to match the final Python candidate bytes.
That was measured, not required for acceptance or promised across FFT platforms.
Python and its numerical packages remain developer-only qualification tools.

Retention review caught incorrect compiler labeling in the first standalone
native audit: its recorded environment and executable used Homebrew Rust 1.98.0,
despite the intended toolchain setting. That run remains intact with a correction.
The same frozen corpus and independent audit were repeated with verified rustup
Rust 1.97.1 and the repository release profile. All 25 finite checks, 13 shuffled
comparisons and 14 unchanged controls passed again; every PCM and gain file
matches the earlier run exactly. The corrected executable is
`0d805acc07e0f328c7a7d007da5faf8be148d9be5af6859e41b51c741831a10b`.
The two pipeline benchmark binaries independently retain the Rust 1.97.1 compiler
identity, so their compiler claims and measurements did not require a rerun.

## Retained failures

The initial unconditional −1.2 dB guard passed the producer and BS checks but
failed the independent finite bank on six inputs; the worst result was
−0.822340 dB. Unconditional −1.7 dB passed all 25 finite checks but unnecessarily
changed 118 ending samples of an already compliant long 0.8 tone, by up to
0.112363 dB. Triggering the guard only above the product ceiling restores those
bytes while retaining all 25 finite passes.

Six broader complete-sinc diagnostics remain above the ceiling:

| Retained output | Numerical upper, dB |
| --- | ---: |
| Rapid mask 0.8 | +3.223018 |
| Rapid mask 16 | +3.482565 |
| Alternating 16 | +6.655904 |
| Near-Nyquist tone | +4.475527 |
| Earlier-processed failure 16 | +3.168739 |
| Long rapid mask 16 | +4.901100 |

These are retained calculations on candidate outputs whose bytes match the native
outputs, not new native diagnostic sweeps. They remain named stress failures under
a broader reconstruction model. They do not become an unbounded requirement to
prove every converter, nor does passing finite probes establish such a guarantee.
The numerical upper estimates are not formal interval certificates.
Independent 80-digit direct sums at the six reported coordinates confirm points
above the ceiling on these same output bytes. Those point checks do not certify
the global upper estimates.

## Source preparation cost

The pipeline harness uses actual 196608-frame, 48 kHz stereo signed-16 WAVs through
verified `AudioSession`, `PreparedSource`, `StageAudio` and `LimitedAudio`. It
includes ordinary tone, maximum signed-16 samples alternating with zero, and a
3/2 Preserve followed by RoomTone and Source. No substitute PCM provider or
post-read gain was used. An earlier float32 WAV attempt was explicitly rejected
by the existing signed-16-only WAV admission and remains retained. Amplitude 16
is therefore a separate pure-kernel test, not the admitted-source pipeline case.

On Apple M5 Max, macOS 26.5.2 and Rust 1.97.1, the harness matches the repository's
release profile. Each batch contains 8192 samples, or 170.667 ms of audio. The
table compares eleven successive batches with complete required context:

| Case | Before bus cache, mean ms | Cached mean / maximum ms | Cached batches over audio duration |
| --- | ---: | ---: | ---: |
| Ordinary tone | 143.650 | 111.933 / 127.519 | 0/11 |
| Maximum signed-16 / zero | 182.343 | 147.604 / 174.395 | 1/11 |
| Preserve and RoomTone | 143.055 | 108.347 / 143.425 | 0/11 |

The hot outlier occurs at the first context-alignment transition. Its following
ten batches average 144.925 ms and peak at 150.872 ms. Source admissions fall from
261 to 44 per ordinary/hot batch, including revalidation. Cached full-tile reads
take about 11 microseconds. All 135 compared result hashes and fifteen complete
artifact files match the uncached baseline; cold shuffled reads and separately
prepared bus and kernel output also match exactly. Residency stays within twelve
bus entries and four limited tiles. The cached process peaks at 22249472 bytes RSS.

Cold interior preparation remains 143.198 ms ordinary, 180.535 ms hot and
219.077 ms Preserve/RoomTone. Source verification, decoding and indexing are
separately measured at roughly 62–74 ms. These results retain the need to prefill
before activation and handle starvation explicitly. They do not prove sustained
device deadlines for every source or edit.

The baseline ran during other workspace checks, with a starting load average of
5.90; the cached run paused builds and started at 3.33 on 18 logical CPUs. Timing
differences are therefore not a perfectly isolated causal comparison. The exact
byte comparisons and reduced source-read counts independently establish reuse.
Default-profile preliminary timings are retained separately and are not reported
as application release measurements.

## Native interaction and repository checks

The optimized Rust 1.97.1 application opened the retained five-beat Pause review
project: 120 Original frames, 187 edit frames at 30000/1001 fps. Space started
playback, which ended at exactly 187/187 with the final seven-frame Hold selected
and Original counter 119 displayed. `12h` moved back to boundary 175 and picture
frame 176. `:monitor 25%` showed a 0.25 slider value while the project stayed Saved;
the initial 12.5% level was restored before another playback. Immediate Space
then Space cancelled preparation at boundary zero. This does not separately
establish a midstream pause on this build.

The [saved interface target](../design/README.md) remains the comparison point:
the picture dominates, the Original stays pinned, lavender distinguishes
selection/focus and yellow marks the cursor. The Limited audition label is clear
beside Play edit and its Space key. `?` opened the updated guide, PageDown scrolled
it, Escape closed it, and Tab/Shift-Tab moved between Viewer and Inspector. The
tool's truncated accessibility tree did not expose the help content; appearance
and paging were checked visually. VoiceOver, CJK IME, physical keyboard layouts,
acoustic listening and route/sleep/wake stress remain unqualified here.

Command-Q exited the owned app. Its logical SQLite dump remained identical.
The review executable matched the built binary:
`f68cb731edbc6ae1c3e7e4ce416dac7aa158156d61beba80b12d059bcbc3e5b9`.

Formatting, locked workspace Clippy with warnings denied, locked workspace
tests/build and CLI doctor all passed. **1,265 tests passed, zero failed, zero
ignored.** ASan/UBSan also passed for the fixed native peak-bank admission and
ownership probe. Eight design image/prompt pairs, all five archived specification
files and the historical PDF copy passed their identity checks.

[Application evidence](../../tools/audio-limiter-qualification/evidence/2026-09-24-limited-app/README.md)
retains raw command results, source hashes, native observations and earlier
failures. The gate runner includes crate Markdown in its hash set, so fixing
two missing README spaces made its all-file freeze flag false. All executable
sources and fixtures were unchanged; the retained byte comparison isolates that
one documentation change. The later optimized build held all 400 selected paths
unchanged. No implementation changes followed verification.

Two earlier full runs failed on ten-second test-reference deadlines. Those
references now use the existing sixty-second production preparation budget;
the production deadline was not increased. The first failed run's original
counter omitted its failed test binary; a retained correction records 988 passed
and one failed. Raw logs and unsuccessful reports remain intact.

## Still required

Voice treatments, sends, the complete group mix, integrated output reporting,
encoded export and decoded-file verification remain open. Listening must assess
gain motion, guard-threshold behavior and preservation of reactions and pauses.
The numerical and preparation harnesses do not qualify acoustic output, physical
converters, GUI aesthetics or the entire native playback lifecycle. No completed
master, universal peak theorem or release readiness is inferred from these results.
