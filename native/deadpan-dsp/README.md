# Canonical DSP bridge

`CanonicalStretch` prepares owned planar 48 kHz stereo PCM on one worker. Its
fixed schedule is available for shared preview and export preparation. It does not provide
playback, devices, a cache, source decoding, resampling, channel conversion,
mixing, fades, or a limiter. Never call its constructor, reads or replay from an
audio callback.

Input has 1 through 1,048,576 frames (8 MiB of retained f32 samples), matching
finite channels and magnitude at most 16.0. The peak limit rejects unsuitable
input without normalization or clamping. Legacy `CanonicalRecipe::new` derives
the exact rate from its input/output counts, within 1/8 through 8.
`CanonicalRecipe::with_rate` takes a reduced `StretchRate` independently of these
counts, with positive output bounded to 8,388,608 frames. Its unsigned 64-bit
rational rate has the same limits; integer pitch is within -24 through +24
semitones in both modes. Reads accept at most 256 frames. Configuration owns
additional native DSP state; the retained-input bound is not a process-memory
or allocation guarantee.

`StreamRecipe` and `StreamingStretch` supply the exact-rate schedule through
bounded call-scoped windows, with positive logical u32 lengths. Call
`required_input(requested)` before each read and provide that exact range from
the same immutable signal. The largest admitted window is 65,536 stereo frames;
later calls need at most 2,048. No input pointer survives the call, and invalid
or cancelled supplies do not advance DSP. The host separately bounds total
work, validates unused input and owns output storage. The shared stage worker
uses this path without changing the schedule or its engine ID.

`FixedPeakBank` is a separate worker-side finite FIR detector, not a limiter. It
copies a bounded 15-by-129 f64 coefficient table at construction and prepares
the portable Signalsmith real-FFT kernels once. Each call consumes exactly 1152
stereo f32 frames (a 1024-frame tile with 64 real context frames on either
side) and returns 1024 f64 magnitudes linked across phases and channels. The
bank uses the shared engine version `FIXED_PEAK_ENGINE_ID`; callers must also
include the actual coefficient table and source identity in any cache key.
Cancellation is checked before and after each tile, and a rejected or cancelled
call leaves its output unchanged. Each call is bounded but is not a realtime or
wall-time guarantee. The native plan is worker-owned, retains no caller buffers,
and performs no heap allocation while processing a tile.

`FiniteStereoFilter` evaluates signed stereo through a copied 4,097-tap f64
kernel using the same pinned portable FFT. One call takes 5,120 frames per
channel: 2,048 preceding samples, a 1,024-sample core, and 2,048 following
samples. It returns the two signed core channels without limiting or crossfeed.
Coefficients run from offset -2,048 through +2,048 and have magnitude at most
16; input is finite with magnitude at most 1e100, leaving bounded FFT headroom.
Both native and safe Rust boundaries preserve both outputs on failure. Native
processing retains no caller arrays or signal history and allocates no heap
memory. Cancellation is checked before and after one tile.

Callers must use a fixed grid in the effect's retained clock, supply qualified
adjacent context and enforce each voice's silence/tail policy. For a symmetric
kernel the 2,048-frame group delay is compensated by the supplied look-ahead;
no output timestamp is shifted. At 48 kHz this requires 42⅔ ms of past and
future context. The adapter deliberately neither fabricates missing samples
nor determines owner routing. `FINITE_FILTER_ENGINE_ID` versions its numerical
shape; cache identity must also retain coefficients, source and clock provenance.
The [finite-filter qualification](../../docs/qualification/finite-filter-2026-10-11.md)
covers this processor. The audio crate now supplies the
[authored EQ](../../docs/EQUALIZER.md) consumers.

`CausalStereoFilter` copies separate left/right kernels of 1 through 384,001
coefficients. Lag zero is current input, with no look-ahead. Each read supplies
`taps - 1` history samples plus 4,096 core samples per channel. It preserves
signed f64, output labels and channel independence. Complete fixed-grid context
makes cold and warm reads identical on one qualified build/machine. Native
construction owns the FFT buffers; processing allocates nothing. Both boundaries
validate lengths/finite bounds and preserve both output arrays on failure.
`native_bytes_bound` gives a conservative per-plan bound; caller arrays and
aggregate graph residency need separate admission. The
[finite wet effects](../../docs/FINITE_WET_EFFECTS.md) use this numerical adapter.
Continuous authored returns and incoming-tail policy remain unimplemented.

Cancellation is cooperative between reads. The first read also runs the fixed
negative-context priming schedule, up to 203 discarded quanta at the slowest
admitted rate, and is not interrupted mid-native-call. Later reads may span an
existing prepared block and one new block. Replay loops over bounded reads,
preserving completed progress on cancellation. Backward seek requires a new
renderer or an external prepared-PCM cache. Full-origin replay work grows with
the target and is not an interactive seek claim.

`src/canonical.hpp` is the same implementation measured by the original
[canonical qualification](../../docs/qualification/audio-canonical-2026-09-20.md).
Its schedule, signed nearest-even input boundaries, seed 1337, dense 120/15 ms
window, zero extension and exact cropping are unchanged. `ENGINE_ID` versions
these semantics. Cache callers must also bind source bytes, selected interval,
interpretation and the entire `CanonicalRecipe`; the ID is not an asset hash.
Explicit-rate recipes use `EXACT_RATE_ENGINE_ID`. Absolute nearest-even input
boundaries use int128 arithmetic, including negative priming context. Equivalent
rates normalize in Rust and C++ before the approximate upstream latency setup.
The origin remains zero: fractional phase needs an explicitly prepared sampling
grid, not another offset rounded inside the stretcher.

The safe Rust wrapper retains immutable boxed input until after native destroy,
limits every call, and permits no Rust callback across the C ABI. The native
adapter rechecks admission and output limits, stages output so errors leave
caller buffers untouched, rejects nonfinite output, and poisons failed engines.
All exported C functions are exception boundaries. The object stays on the
thread where it was constructed; its owned input can be sent to that worker.

## Build and checks

The Cargo build compiles the retained C++17 headers with `-O2`, warnings denied,
and the portable FFT. Builds do not download sources. Fast-math and external FFT
backend switches are compile errors. Actual reference qualification is on Apple
Silicon macOS; the macOS 15 deployment target is not proof of OS coverage.

```sh
cargo test --locked -p deadpan-dsp
cargo clippy --locked -p deadpan-dsp --all-targets -- -D warnings
```

`tests/canonical-sha256.txt` captures all 50 preview output hashes from the
previously committed `tools/audio-qualification/canonical-report.json`. The Rust
test reads the frozen mixed fixture, recreates the exact short impulses, verifies
the mixed input hash, and requires every bridge output hash to match. Recreating
the trigonometric fixture in Rust initially produced a different input hash;
compiler/libm floating-point differences must not change the regression's input.
The retained bytes and their provenance are in `tests/fixtures`. Tests also cover irregular reads, exact replay,
cancellation/resume, output suffix ownership, source ownership after moves,
invalid inputs and vendored source integrity. These are numerical regressions,
not listening or device qualification.

The explicit-rate tests run all 50 historical hashes again through the new
constructor. They also verify independence from output length and zero-padded
input storage, partition/replay equality, normalized identity and u64 admission.
Two rates on opposite sides of 257/512 have the same f64 representation but
different exact half-sample boundary decisions; actual PCM must distinguish them.

The standalone ABI probe can instrument both the new C++ bridge and the retained
header-only DSP. Run from the repository root, using a task-specific scratch
directory outside the checkout:

```sh
mkdir -p /tmp/deadpan-dsp-abi
clang++ -std=c++17 -O2 -g -Wall -Wextra -Werror -arch arm64 -mmacosx-version-min=15.0 -fsanitize=address,undefined -fno-omit-frame-pointer -fno-sanitize-recover=all -Inative/deadpan-dsp/src -isystem native/deadpan-dsp/vendor/signalsmith-stretch/include -isystem native/deadpan-dsp/vendor/signalsmith-linear/include native/deadpan-dsp/src/adapter.cpp native/deadpan-dsp/tests/abi_probe.cpp -o /tmp/deadpan-dsp-abi/abi-probe
/tmp/deadpan-dsp-abi/abi-probe
```

The probe covers create/read/destroy, null arguments, rejected lengths and
nonfinite/over-limit inputs, output guards, untouched suffixes, EOF, empty reads,
repeated lifetimes, and admitted extreme rates/pitches. It performs no device I/O.
Compile `tests/exact_rate_probe.cpp` with the same flags and adapter to exercise
the new ABI, u64 rate arithmetic, reduced-equivalent ratios, count independence
and output guards. AddressSanitizer and UndefinedBehaviorSanitizer are supported
on the measured macOS environment; its LeakSanitizer option is unavailable.

Compile `tests/finite_peak_abi_probe.cpp` with `src/finite_peak.cpp` and the same
flags and include paths to exercise the detector's fixed ABI lengths, coefficient
and sample admission, direct impulse response, output guards and independent
plans.

Compile `tests/finite_filter_abi_probe.cpp` with `src/finite_filter.cpp` and the
same sanitizer flags to exercise signed output guards, all creation allocation
failure sites, rejected lengths/pointers/samples and allocation-free reads.
`cargo build --locked --release -p deadpan-dsp --example finite_filter_probe`
builds a standalone numerical/performance fixture. Run its binary only after
compilation finishes; its measurements exclude source preparation, group mixing,
device output and editor latency. See the qualification record for actual runs.

Compile `tests/streaming_probe.cpp` with `src/streaming.cpp` and the same
sanitizer flags to compare bounded input against the original complete-source
interface, including a two-minute input, irregular reads, extreme rates,
invalid windows, nonfinite input, output guards and EOF. See
[streamed-input qualification](../../docs/qualification/streamed-input-2026-10-11.md).
