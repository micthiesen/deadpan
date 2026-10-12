# Finite wet-effect processors, 2026-10-12

`FiniteWetEffect` now implements deterministic finite reverb and delay with
complete declared history. `CausalStereoFilter` supplies the owned native
stereo convolution. This qualifies the numerical foundation for continuous
returns; no authored mixer path or new UI control consumes it yet. Existing
selected sends continue to feed allocated pauses. DP-09, DP-16 and the full
product goal remain open.

The [processing contract](../FINITE_WET_EFFECTS.md) defines four-second reverb,
eight-second delay, zero latency, a 4,096-sample output tile and a final 50 ms
kernel taper. Delay contains 26 exact echoes at 300 ms intervals and half
feedback. Reverb retains the existing independent stereo comb/all-pass network.
The finite recipes have new algorithm identities and do not replace the
existing recursive Hold-tail algorithm. No timeline duration, stored recipe,
dependency or device behavior changes in this increment.

## Verification and review

All 590 audio tests and 32 native DSP tests pass, without ignores. Fifteen new
tests cover the independent causal kernels, direct convolution, full-history
orientation, both output boundaries, DC/Nyquist packing, copied coefficients,
stereo isolation, signed wide values, exact silence, declared echo positions,
reverb taper, shuffled/cold reads, cancellation, invalid input and memory bounds.
Existing canonical stretch hashes, finite EQ, peak-bank, sound-owner, selected
send, Hold, limiter and timing-representation regressions remain passing.

Strict workspace/all-target Clippy with `ui-harness`, formatting, whitespace and
the release qualification build pass. The final iterator-only cleanup was
followed by all 32 DSP tests and all seven finite-wet tests. The primary agent
reviewed the complete patch, boundary safety, output staging, memory accounting,
convolution orientation and tests, working alone at the user's instruction.

The raw C ABI witness covers eight kernel sizes up to 384,001 taps. In both the
optimized and final ASan/UBSan builds, all 112 injected allocation failures
return null handles and restore the live allocation/byte counts. All 96 valid
tiles preserve output guards, allocate nothing while processing and release all
tracked allocations. Invalid pointers, lengths, coefficients and samples are
refused before processing. Sanitizers ran with `-fno-sanitize-recover=all`.
macOS LeakSanitizer was not used; the explicit allocation witness provides the
allocation-lifetime evidence.

Three earlier failures are retained, rather than counted as passes:

- The initial qualification example used `LowerHex` on this SHA-256 version's
  result type. Compilation refused it; the final version formats digest bytes.
- Clippy rejected three index-based loops. Equivalent iterator loops replaced
  them, with no suppression.
- The initial probe's replacement allocator used `alignof(max_align_t)`, which is
  8 on this toolchain, although ordinary C++ `operator new` requires 16-byte
  alignment. UBSan caught a misaligned constructor. That process exited zero
  despite the diagnostic and is recorded as a failed check. The probe now uses
  `__STDCPP_DEFAULT_NEW_ALIGNMENT__`; the final fatal-sanitizer run is clean.
  Production uses the standard allocator and had no corresponding change.

## Measured cost on this Mac

Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1,
Apple Clang 21.0.0. Three fresh release processes each construct both effects
and read a repeated tile 101 times. These runs had no concurrent owned compiler
or audio-device test; system caches were not purged. Each run also checks 20
samples per effect against direct f64 convolution and compares cold/warm output.

| Process | Effect | Construction | Tile p50 | Tile p99 | Slowest tile |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | Reverb | 7.672 ms | 5.170 ms | 5.285 ms | 5.295 ms |
| 1 | Delay | 10.368 ms | 10.464 ms | 10.584 ms | 10.638 ms |
| 2 | Reverb | 7.786 ms | 5.170 ms | 5.270 ms | 5.302 ms |
| 2 | Delay | 10.259 ms | 10.444 ms | 10.699 ms | 10.702 ms |
| 3 | Reverb | 7.611 ms | 5.167 ms | 5.256 ms | 5.274 ms |
| 3 | Delay | 10.269 ms | 10.433 ms | 10.720 ms | 10.720 ms |

Maximum direct error was 8.881784197001252e-15 for reverb and
3.3306690738754696e-16 for delay, below the 2e-12 absolute test bound. All three
runs produced identical kernel, input and output hashes. Cold/warm output was
bit-identical on this build and machine. The timed reads include public sample
validation and output staging. They exclude media I/O, history preparation,
owner routing, silence policy, group mixing, limiting, scheduling and the UI.
No application latency, realtime or listening claim follows.

The raw native allocation witness measured 23,659,040 bytes for the reverb-sized
plan and 47,252,000 bytes for the maximum delay-sized plan. The corresponding
native bounds are 34,603,008 and 68,157,440 bytes. The audio API admits temporary
kernel/network storage too: 38,723,600 and 75,350,032 bytes. Caller context and
output buffers separately require 3,203,072 and 6,275,072 bytes. These are
per-processor figures; aggregate graph admission is still required.

## Remaining work and evidence

Integrate event and owner returns before their parent treatments. Preserve
shared nonlinear drive, retained input clocks and the consuming sample's
per-voice incoming-tail permissions. Admit total context, native plans and
cached tiles under existing work/memory/deadline limits. Qualify complete stored
recipes, captured native controls, Repeat/copy/Preserve behavior, cold playback,
loops and emitted-file output. Do not append a global wet signal after the root
or weaken partial sound-owner scope refusals to make those cases pass.

No live GUI, audio device, physical listening or new export was run for this
numerical increment. Those feature-level checks remain owed with integration.
Commands, full logs, source patch, machine details, actual binary/source hashes,
allocation reports and all three processor reports are retained in
[`2026-10-12-finite-wet`](../../tools/audio-qualification/evidence/2026-10-12-finite-wet/).
Run its `check-evidence.py --source` to audit retained results and current sources.
