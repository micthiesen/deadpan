# Finite reverb and delay preparation

`deadpan_audio::FiniteWetEffect` prepares wet-only stereo tiles with complete,
finite input history. This is the numerical basis for specification §10.2's
continuous returns and §17.4's random access. Authored `wet_sends` on a beat or
group now use it through the shared contribution mixer. Native
`:wet-send reverb|delay -6dB|off` changes the captured beat or isolated Repeat
play; whole-beat changes support Undo, dot and macros. The inspector shows both
levels. Existing `tail_sends` separately feeds allocated
[tail pauses](REVERSE_AND_TAILS.md); enabling a continuous return adds no time.

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

After successful construction, `FiniteWetEffect::retained_bytes_bound` keeps the
complete existing native bound. The Rust coefficient vectors and impulse network
have dropped; their separate constructor allowance no longer occupies the graph
ledger. Reverb admits 38,723,600 bytes while constructing and retains a bound of
34,603,008 bytes; Delay admits 75,350,032 and retains 68,157,440. Never substitute
the retained bound for constructor admission or release native FFT/kernel/work
arrays, which stay alive. These are owned-storage bounds, not an RSS limit.

On one qualified build and machine, identical fixed-grid context produces
bit-identical output after linear playback, shuffled reads or fresh construction.
The processor does not define arbitrary output partitioning: consumers read the
same absolute tiles and then slice them. Different compiler/CPU floating-point
results are subject to the documented numerical tolerance, not a bitwise claim.

## Owner returns

Each owner's send taps its complete ordered chain, including balance. The
resulting wet signal joins that owner's dry signal before parent treatments.
Child returns may feed a parent's effect; an owner's own return cannot feed
itself. Its allocated-Tail send taps before its own continuous return.
Contributions retain their separate automation clocks and selected output reads
retain the shared nonlinear drive of all permitted inputs.

The consuming sample determines permissions for complete temporal history.
A silent Hold excludes the Original return even when an allowed independent
sound remains audible. An explicit Tail provider excludes other incoming
ancestor Original returns, including in a Repeat default gap. It keeps its
selected input/effect, its ordinary ancestor treatments and any effect authored
on the Tail itself. Independent sounds keep their own permissions. Forbidden
Original history cannot enter an allowed sound's wet return through saturation.

The graph caches absolute fixed-grid tiles within one preparation request and
shares one native workspace per effect across nested owners. Workspace creation
follows recursive input preparation. It admits the complete preparation bound
against the shared PCM-equivalent residency budget, then charges the retained
native bound until graph cleanup; temporary history/output
arrays and cached tiles have separate reservations. Deadline, cancellation,
prepared-frame, node-count, depth and plan-work limits remain active. A refusal
does not publish partial PCM. This establishes bounded preparation, not live
playback performance or persistent cache reuse.

## Independent sound returns

`SoundEvent` and `BeatSound` store separate reverb and delay levels in
`treatments.wet_sends`, beside the complete ordered per-event recipe.
The placed-sound inspector teaches `:sound-wet-send reverb|delay -6dB|off`;
the captured target, typed writer transaction and durable Undo preserve the
complete source recipe, route, placement, gain, edges and permissions.

Each event filters its complete retained input after time/pitch mapping,
authored input edges, gain envelopes, EQ, saturation and balance in their stored
order. Automation uses local frames from the complete sound onset. Saved delivery
routes move the processed samples afterward. Inserted route gaps do not advance
the retained effect clock. Current consuming gates and their edge fades bound
the returned signal without adding audible time. An explicit Tail pause remains
the way to allocate additional time. The event's allocated-Tail send taps dry
input after the event's complete treatments, excluding its own continuous return. Parent treatments
and sends receive the dry sound plus its return.

Event tiles share the owner graph's native workspaces and aggregate node,
prepared-frame and residency limits. Their cache is private to one immutable
preparation request: graph prefix plus voice identifies the checked input, with
effect, absolute tile, allocation and input envelope in the key. No event tile
is reused across revisions or independent preparation requests.

Core 53, store 81 and frozen audio context 12 retain these recipes.
Older unused development packages are refused without modification under the
owner's format-breaking authorization. Event copy/recopy through nested Preserve
stages is checked; broader retained/loop and incoming-tail qualification,
partial attached sound-owner processing scope,
persistent filtered caching and the rest of the
voice/mastering graph remain agent work. DP-09 and DP-16 remain partial.

The [qualification](qualification/finite-wet-2026-10-12.md) records the numerical
tests, native allocation and sanitizer witness, measured cost, exact source
identity and current integration limits.
