# Finite stereo filter processor, 2026-10-11

`deadpan-dsp::FiniteStereoFilter` implements bounded signed stereo FIR processing
for the required EQ/filter stage. This qualifies the numerical adapter, not the
complete editor feature. No authored recipe or control currently exposes it;
existing playback/export still use the existing treatment chain. DP-09 and
DP-16 remain partial, with the full EQ/filter integration still owed.

## Processing contract

The adapter copies one finite 4,097-tap f64 kernel at construction and processes
1,024 stereo frames per call. The caller supplies 2,048 real adjacent frames on
each side, on a stable grid in the effect's clock. A symmetric kernel retains
the original sample coordinates by compensating its group delay with look-ahead.
The context is 42⅔ ms on either side at 48 kHz. Signal history is not retained
between calls, so an identical input tile produces identical output after a
seek, reuse or fresh construction on this qualified build.

The pinned portable Signalsmith `RealFFT<double>` uses a 5,120-point transform.
The coefficient order is explicitly input offsets -2,048 through +2,048; the
implementation reverses the kernel and discards the first 4,096 circular
convolution samples. The remaining core is the alias-free overlap-save region.
DC and Nyquist retain the library's separate real-bin packing. This uses the
standard finite convolution method described in the
[overlap-add/overlap-save analysis](https://arxiv.org/abs/2302.08845).

Stereo remains signed f64 without crossfeed, normalization or clipping. The
coefficient bound is ±16; the finite input bound is ±1e100, leaving substantial
headroom in the fixed FFT and products. Construction owns all work space.
Native reads allocate nothing and retain no Rust pointer. The handle stays on
its construction worker. Rust and C++ each validate their boundary; errors and
cooperative cancellation leave both caller output arrays unchanged. Cancellation
is checked around the bounded tile, not inside the FFT.

## Verification

All 24 native DSP tests pass, including seven new filter tests. They compare
both channels against independent direct f64 convolution with an asymmetric
kernel, exercise the outermost context samples and both core boundaries, verify
DC/Nyquist scaling, negative impulses and no crossfeed, and compare five tiles
bit for bit after shuffled seeks and fresh construction. They also check copied
coefficients, independent plans, exact silence, numeric bounds, invalid lengths,
nonfinite input, cancellation and reuse after rejection. Existing stretch hashes,
streaming schedule and peak-bank tests still pass.

The direct C ABI probe passes AddressSanitizer and UndefinedBehaviorSanitizer.
It injects failure at all 15 constructor allocation sites and returns to the
original live-allocation count each time. It then processes 100 tiles with no
new allocation, guards both output arrays and rejects null pointers and wrong
lengths before dereferencing. macOS LeakSanitizer is not available; the explicit
allocation witness and ASan/UBSan are the evidence, not a LeakSanitizer claim.

The primary agent reviewed the complete source and tests without subagents.
Initial compilation caught the current SHA-256 result type lacking `LowerHex`;
the probe now encodes each digest byte explicitly. Strict Clippy caught three
index-based test loops; they now enumerate paired channels. Final audio tests,
Clippy, formatting and whitespace results are retained with the evidence. All
516 audio tests pass. Review also clarified that the two immutable input arrays
may alias; the final release measurements follow that comment-only correction.

## Measured processor cost

Apple M5 Max, 18 physical cores, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1.
Three fresh release processes each construct a 4 kHz Blackman-windowed sinc
fixture and process the same warm tile 5,000 times. The fixture follows the
[windowed FIR construction](https://www.dsprelated.com/freebooks/sasp/Window_Method_FIR_Filter.html).
It is a numerical fixture, not a chosen product EQ design. No compiler or
device measurement ran concurrently. The operating-system cache was not purged.

| Process | Construction + first tile | Tile p50 | Tile p95 | Slowest tile |
| --- | ---: | ---: | ---: | ---: |
| 1 | 115.291 µs | 48.916 µs | 52.208 µs | 83.666 µs |
| 2 | 107.041 µs | 48.916 µs | 52.125 µs | 131.875 µs |
| 3 | 98.625 µs | 48.833 µs | 52.000 µs | 79.291 µs |

Every process compares all 2,048 signed output values against direct finite
convolution; maximum absolute error is 1.7763568394002505e-15. The timed runs
include public validation and transactional staging. They exclude media I/O,
context preparation, owner routing, group mixing, limiting, device scheduling
and the GUI. They establish kernel cost on this Mac, not application latency or
listening quality. CPU/file caches are warm during the repeated tile sequence.

## Remaining integration

Define a useful bounded authored filter recipe and order, connect its typed
commands and captured native controls, and qualify its frequency response.
The shared audio path must retain complete filter input through cuts, copies,
Repeat instances and historical clocks, admit total work/memory before media,
and suppress incoming filter tails per voice without silencing independently
permitted sounds. A halo of current physical timeline samples alone is not
sufficient evidence for retained-owner correctness. Preview, random access,
loops and export must share that implementation and its cache identity.

Commands, logs, binary/source hashes, sanitizer results and the three processor
runs are retained in
[`2026-10-11-finite-filter`](../../tools/audio-qualification/evidence/2026-10-11-finite-filter/).
