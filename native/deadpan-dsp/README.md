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
