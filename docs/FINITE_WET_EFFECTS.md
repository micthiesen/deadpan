# Finite reverb and delay preparation

`deadpan_audio::FiniteWetEffect` prepares wet-only stereo tiles with complete,
finite input history. This is the numerical basis for specification §10.2's
continuous returns and §17.4's random access. The authored mixer does not yet
consume it. Existing selected sends still feed only the allocated
[tail pauses](REVERSE_AND_TAILS.md); their sound and stored recipes are unchanged.

## Exact processing contract

Every sample is on the 48 kHz mix grid. The caller supplies all history followed
by a 4,096-frame output interval. Each channel is filtered independently in f64.
Coefficient `h[j]` weights the input at lag `j`; there is no look-ahead or output
timestamp shift. No dry signal, normalization, limiter or crossfeed is added.

| Effect | Declared history and tail | Response |
| --- | ---: | --- |
| Reverb | 192,000 samples, 4 s | Existing stereo eight-comb/four-all-pass network's impulse response, with independent channel delay lengths. |
| Delay | 384,000 samples, 8 s | 26 echoes at exact 14,400-sample intervals; amplitudes 1, 1/2, through 2^-25. |

The impulse kernels taper linearly over the final 2,400 samples to exact zero.
The delay has no echo in that last 50 ms, so its last nonzero coefficient is
at 374,400 samples. These are explicit finite effect recipes with new identities,
not claims that the old recursive network has mathematically finite support.
`wet_effect_descriptor` declares latency zero, the history/tail bounds and tile
size. The supplied context must include real qualified history, or policy-authorized
zero extension. A tail bound never grants extra authored time.

Algorithm identities are
`deadpan-wet-reverb-comb8-allpass4-48k-fir4s-fade50ms-v1` and
`deadpan-wet-delay-300ms-half-48k-fir8s-fade50ms-v1`. Any prepared cache must also
include the native `CAUSAL_FILTER_ENGINE_ID`, source bytes and interpretation,
complete input recipe, effect owner clock, immutable plan and contribution
permissions. The algorithm string alone is not a reusable signal identity.

## Native boundary and admission

`deadpan_dsp::CausalStereoFilter` copies two equal kernels of 1 through 384,001
finite coefficients, bounded to ±16. It consumes exactly `taps - 1 + 4096`
finite samples per channel, bounded to ±1e100, and publishes both outputs only
after successful completion. Signed wide values survive for later group mixing
and limiting. The safe boundary rejects wrong lengths and invalid values before
FFI. Native code checks its own contract, catches exceptions and stages output.
Invalid calls do not poison a plan; native processing failures do.

The pinned portable Signalsmith real FFT uses the next power of two covering
the complete input. Lag-ordered convolution discards the initial history region;
DC and Nyquist are multiplied as separately packed real bins. Construction owns
all native work buffers. Processing allocates nothing and retains no input
pointer or signal state. The processor is neither Send nor Sync. Use one
preparation worker, never an audio callback. Cancellation is checked during
kernel construction and around each bounded native tile.

`CausalStereoFilter::native_bytes_bound` admits the plan and FFT arrays before
construction. `FiniteWetEffect::preparation_bytes_bound` also includes temporary
kernels and the impulse network. Caller-owned context, outputs and graph caches
need separate admission. This is a per-processor bound, not aggregate application
memory admission or a realtime guarantee.

On one qualified build and machine, identical fixed-grid context produces
bit-identical output after linear playback, shuffled reads or fresh construction.
The processor does not define arbitrary output partitioning: consumers read the
same absolute tiles and then slice them. Different compiler/CPU floating-point
results are subject to the documented numerical tolerance, not a bitwise claim.

## Remaining mixer integration

Returns must join the correct voice or owner bus before its parent treatments.
Appending every return after the root would bypass parent EQ and saturation.
The shared graph must preserve each contribution's current and retained clocks,
process the complete permitted prefix, and gate incoming tails by the consuming
sample's silence or explicit Tail policy. Selected output must retain shared
nonlinear drive. Event sends and owner sends need distinct insertion boundaries;
no return may implicitly feed itself.

Admit aggregate history work, native plans, temporary arrays and cached tiles
through the existing deadline/depth/memory budgets. Preserve atomic refusals for
unsupported partial sound-owner scope. Then qualify stored recipes, captured
native authoring, cold seeks, Repeat/copy/Preserve clocks, loops and emitted files.
These are still agent work. DP-09 and DP-16 remain partial.

The [qualification](qualification/finite-wet-2026-10-12.md) records the numerical
tests, native allocation and sanitizer witness, measured cost, exact source
identity and current integration limits.
