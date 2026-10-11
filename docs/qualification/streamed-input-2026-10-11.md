# Continuous Preserve with streamed input, 2026-10-11

Preserve and fixed-pitch stages previously copied their entire input into
interleaved and planar buffers before processing. The owned native adapter's
1,048,576-frame limit also rejected authoring beyond about 21.8 seconds.

The new streaming adapter retains one canonical processor and borrows only
the exact next input window. It does not reset the processor, change the
256-frame schedule, approximate a seek or alter the engine ID. The existing
owned-input API remains available with its original bounds.

## Implementation and limits

`CanonicalStretch::required_input` computes the union needed for initial
negative-context priming and the next output quantum, or the next rolling
quantum. The source view zero-extends outside the logical signal. The admitted
window never exceeds 65,536 stereo frames; subsequent windows need at most
2,048. C++ validates the exact range, finite samples and peak before processing,
checks every source access against the borrowed window, stages output until
success and clears borrowed pointers on all return paths. Rust owns the handle
on one thread and supplies no callbacks.

`StageAudio` reads each window through the existing signal/dependency path.
It reserves simultaneous interleaved/planar input storage and the retained
output, preserves nested preparation and re-admission, and scans any input
remaining after a shortened output. The latter cannot hide a missing source,
invalid PCM or dependency. Cancellation and deadlines remain cooperative.
Non-streamed Hold preparation retains its separate input limit.

The worker and edit commands agree on 16,777,216 logical input frames,
8,388,608 retained output frames and 16,777,216 combined input/output frames
of preparation work. The combined budget counts all newly prepared stages
in a read. A valid individual edit can therefore still exceed a nested
preparation's cumulative budget. At 48 kHz, output remains limited to about
174.76 seconds, and combined work to about 349.53 seconds. This is input
streaming, not unbounded or disk-backed output.

## Verification

Apple M5 Max, 18 physical cores, 128 GiB; macOS 26.5.2 (25F84), Rust 1.97.1.
Native media uses the qualified FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 prefix.

- The complete locked suites pass **1,629 tests**: 492 audio, 1,048 core,
  17 DSP and 72 playback, with none ignored.
- Owned-input comparisons match every stereo float at rates 1/8, 8, 1, 3/2
  and 1000/1001, pitches -24, 0 and +24, and both regular and irregular reads.
  Existing legacy/exact-rate adapter tests continue to pass.
- Invalid windows, nonfinite/excessive PCM and cancellation leave the stream
  reusable without advancing or modifying output. Logical u32 maximum lengths
  do not allocate a complete input or output.
- A direct C++ ABI probe under ASan/UBSan compares **3,030,017 stereo frames**
  exactly with the original complete-source interface. Its 2x case consumes
  a two-minute input; other cases cover extreme and fractional rates, pitch,
  irregular reads, output guards and EOF. The largest observed window is
  **27,968 frames**. LeakSanitizer is unavailable on this macOS.
- A real decoded-WAV integration repeats its first 8,192 samples into one
  5,767,168-frame input (120.149 seconds), then processes it continuously at
  2x to 2,883,584 output frames. Every output sample matches the independently
  mapped integer-WAV reference. Late first demand, random warm reads and a
  second cold renderer pass under a **3,015,680 stereo-frame PCM budget**, less
  than the input alone. The former whole-input reservation would need
  14,417,920 frames. This is a reservation test, not a total-process RSS or
  interactive-latency measurement.
- A work-budget rejection still occurs before source access; the new core
  one-minute Preserve/pitch edit verifies undo and document round-trip.

An additional regression supplies one output sample from a long silent prefix
followed by unavailable media. The processor can emit that sample without the
media, but the host still rejects the unused input's missing dependency and
publishes no cache entry. It passes in the final 14-test focused run, which also
rechecks the long decoded-WAV path, core edit and native window comparisons.

Two witnesses were run on archived commit
`913b4f226d772d40a5294432d3204903b9ed80ec` in a separate Cargo target. The
one-minute core edit fails with its old 21.8-second diagnostic, and the
two-minute late-read audio fixture fails with `Limit("input frames")`.
Archive, overlay and executed-binary hashes are retained. The old hard-cap
test now sets that cap explicitly and continues to prove pre-media rejection.

The rebuilt release CLI also passes a scoped export regression for
`retime-half`, `pitch-shift` and `group-saturation`: **150 pictures and five
audio windows**, all with verified zero sample offset. The minimum block SNR
is 20.944955 dB and maximum level difference 0.480092 dB. These are the existing
short fixtures; the two-minute case above is verified as decoded PCM. The
other 43 export recipes were not run in this focused check.

Release CLI SHA-256:
`41b155c438eb6d3518399349c0f5b65f822dde150393b649b5ff55c16d96b199`.

Strict workspace/all-target Clippy with `ui-harness`, formatting and diff
checks pass. The initial Clippy rejection of a constant-valued test assertion
is retained; the assertion now uses a const block.

[Retained evidence](../../tools/audio-qualification/evidence/2026-10-11-streamed-input)
contains command receipts, logs, source/binary pins, sanitizer results, archived
regression witnesses, actual MP4s with publication-verification reports,
complete fixture packages and self-review notes. Check it with:

```sh
python3 tools/audio-qualification/evidence/2026-10-11-streamed-input/check.py
```

## Scope

Review was performed by the main session without subagents. It covered the
window union, ABI lifetimes and bounds, exact-rate arithmetic, continuous DSP
state, admission of unused input, recursive reservations, cancellation, cache
publication and edit/worker limit agreement.

Disk-backed output, background cache scheduling, full independently timed
group automation, EQ/filtering, arbitrary sends, oversampled shaping and
complete mastering remain open. DP-09 and DP-16 remain partial. This work adds
no live GUI, listening, physical-device or long-export qualification.
