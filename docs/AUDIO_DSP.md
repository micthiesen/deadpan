# Canonical worker DSP boundary

`native/deadpan-dsp` wraps the measured Signalsmith Stretch schedule in a safe
Rust API. The crate vendors the exact pinned Stretch 1.3.2 and Linear 0.3.1
headers and complete MIT notices. Cargo builds C++17 with the portable FFT and
does not download dependencies. Unsafe code is isolated in `src/ffi.rs`.

`StereoPcm` owns two immutable planar 48 kHz channels. Admission requires equal,
nonempty lengths of at most 1,048,576 frames, finite samples, and an absolute
peak no greater than 16.0. The ceiling rejects extreme input; it never normalizes
or clips accepted samples. This bound currently limits a preparation to about
21.85 seconds of input. It is not a transparent streaming solution for long
clips, and arbitrary chunking must not reset the DSP phase.

`CanonicalRecipe` retains positive input/output counts and integer pitch from
−24 through +24 semitones. The input/output ratio is bounded to 1/8 through 8.
`ENGINE_ID` identifies the fixed seed, 120/15 ms analysis configuration,
256-frame production schedule, zero extension and exact origin-based cropping.
Prepared-cache identity will also need original byte identity, selection,
source interpretation and complete recipe; the engine ID alone is insufficient.

`CanonicalStretch` retains the input through native destruction. It is neither
Send nor Sync; transfer owned PCM to a preparation worker and construct the
renderer there. Reads accept equal caller-owned buffers of at most 256 frames.
Internal processing remains 256 frames regardless of consumer partitioning.
The C ABI catches all exceptions, validates lengths and finite output, stages
output before publishing it, and poisons the renderer after a processing failure.
The native source and output pointers never cross a Rust callback.

Cancellation is cooperative before each bounded native read. The first read
also runs a fixed, rate-bounded negative-context priming sequence; it cannot be
interrupted inside that FFI call. Forward `replay_to` checks cancellation between
reads and retains completed progress. Backward seeks need a fresh renderer and
canonical replay or matching prepared PCM. Neither replay nor configuration is
device-callback work. The earlier measured worst read already exceeds a device
deadline; no real-time safety claim follows from allocation-free DSP processing.

The qualification harness includes this production canonical header directly,
so it cannot evolve a second scheduling implementation. Its old normal and
sanitized measurements remain historical evidence, with original source hashes.
Current Rust tests compare prior measured PCM hashes and test consumer partition
and replay equivalence. The focused C++ harness exercises the actual adapter
under ASan/UBSan. See the crate README for commands and fixture provenance.

This adapter does not connect plans to PCM, decode media, infer speaker layouts,
resample, implement room tone/tails/fades/gain/limiting, manage prepared caches,
open output devices, or implement native playback/export. Binding exact source
phase and fractional plan extents to the preparation recipe remains open;
integration must not derive an unintended rate from rounded beat durations.
Fractional pitch, mixed nested pitch-stage processing, reverse and variable-rate
operations remain required. The admitted parameter range exceeds the original measured listening
and quality corpus; full speech/music review and device qualification remain open.
