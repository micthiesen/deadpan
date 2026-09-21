# Exact-rate native DSP, 2026-09-21

This increment separates the canonical stretcher's consumption rate from its
input/output buffer lengths. It is a prerequisite for exact fractional-frame
plan integration, not completion of that integration. The base revision is
`3160844876c3a979b991576811b74be47ae93d66`; its
[macOS CI run](https://github.com/micthiesen/deadpan/actions/runs/35666125413)
passed. No external dependency, vendored header, persisted schema or normative
specification changed.

## Contract and environment

`StretchRate` stores a reduced positive u64 rational in `[1/8,8]`.
`CanonicalRecipe::with_rate` retains this rate independently of the bounded PCM
input length and positive output count. Signed nearest-even input boundaries
are evaluated from the absolute output origin using int128 arithmetic, including
preroll. Native admission checks lengths, ratio and pitch before reading caller
arrays, then normalizes the ratio before approximate latency initialization.
The legacy constructor and engine identity remain unchanged. Explicit-rate
recipes use `deadpan-canonical-stretch-exact-rate-1`.

Testing used Apple M5 Max with 128 GiB memory, macOS 26.5.2 (25F84), Rust 1.97.1
and Apple Clang 21.0.0. The workspace gate used pinned LGPL FFmpeg 8.0.3. Native
probes compiled the production adapter and canonical header with C++17, `-O2`,
ordinary floating point, portable FFT, AddressSanitizer and
UndefinedBehaviorSanitizer. No device or model was involved.

## Results

The exact repository gate passed: formatting, strict workspace Clippy,
**692 Rust tests, zero failures and zero ignored**, workspace build and doctor.
The eleven DSP tests include five new tests covering:

- All 50 historical reference hashes through the explicit-rate constructor,
  in addition to their unchanged legacy-constructor checks.
- Identical output prefixes when the allocated output end changes, and
  identical output when retained input receives only additional zero padding.
  A count-derived reference distinguishes the rate change this prevents.
- Two u64 ratios on opposite sides of 257/512 that become the same f64 value
  yet allocate different exact half-sample boundaries and produce distinct PCM.
- Bit-identical irregular reads, exact replay, cancellation/resume and untouched
  output suffixes under an explicit non-count-derived rate.
- Reduced ratio identity, integer-overflow-resistant admission, scalar limits,
  and independent input/output storage bounds.

Both the existing ownership/error-path ABI probe and the new exact-rate ABI
probe passed ASan/UBSan. The new probe covers null pointers, invalid scalar
bounds before array access, nonfinite/extreme input, unreduced-equivalent ratios,
u64 boundary decisions, allocation independence, guard samples, EOF and admitted
rate extremes. A focused independent arithmetic review compared **234 native
boundaries** with a Python `Fraction` oracle, including negative ties,
u64-scale ratios and output coordinate `2^48+255`; all matched under ASan/UBSan.
These tests do not establish whole-process memory use or realtime latency.

The first sanitizer invocation enabled LeakSanitizer, which this macOS runtime
rejects before the probe runs. The corrected invocation disabled that unsupported
option while retaining ASan and UBSan; the failed setup logs remain in evidence.
The first repository gate stopped on formatting in the new Rust tests; rustfmt
corrected it before the passing gate. No production assertion was weakened.

Raw gates, sanitizer commands/results, independent oracle and source hashes are
retained in [the evidence directory](../../tools/audio-qualification/evidence/2026-09-21-exact-rate/).
The focused arithmetic review found no actionable defects. Independent general
review covered the Rust API, FFI ownership/dispatch, native constructors,
boundary arithmetic, admission, exception handling and ABI tests; it found no
substantiated actionable defects. No GUI or native startup smoke was repeated because
this change does not touch UI, keyboard, focus, lifecycle or device output.

## Integration still required

The exact-rate API starts from a zero-origin input grid. It does not implement
fractional phase by rounding an input offset; the qualified resampler must
prepare that phase. A whole Retime occurrence must process its selected child
signal continuously across cuts, preserving inner DSP history through outer
crops. Mixed Preserve/FollowSpeed stages must retain order. The
[stage preparation contract](../AUDIO_STAGE_PREPARATION.md) records these
requirements.

The 1,048,576-frame native input bound remains, including for intermediate
signals. Long-input preparation, fractional pitch, variable rates/reverse,
complete voice processing, cache lifecycle, listening, device scheduling and
preview/export equivalence remain open. DP-09 and Gates A through C stay partial.
