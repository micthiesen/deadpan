# Audio stage preparation contract

This is the required integration design for the remaining voice graph.
`SequenceAudio` currently renders natural-rate and FollowSpeed source spans;
`CanonicalRecipe::with_rate` provides an exact-rate native stretcher. They are
not yet connected for pitch-preserving plan playback.

## Continuous processing domains

A Preserve Retime around a Sequence processes its selected child signal
continuously across leaf cuts. It must not restart the stretcher for each
`AudioSpan`. Each Retime occurrence is a processing stage with its own stable
origin. Process inner stages before outer stages. Preserve stages retain DSP
history; FollowSpeed stages use the qualified resampler. Overall timing and
steady-tone pitch products do not establish equivalent transient behavior or
intermediate anti-alias filtering.

Let `C = 48000 * fps.den / fps.num`, `[a,b)` be the selected child-frame range,
and `d` the stage's output duration in frames. The exact speed is `s=(b-a)/d`.
Prepare child PCM on the rational grid `X[k]=sampleChild(C*a+k)` so fractional
input phase survives. Preserve consumes this PCM with native boundary
`K(m)=round_even(s*m)`, using unity pitch. FollowSpeed reconstructs at `s*m`.
At 30000/1001 fps, three frames into two has exact speed 3/2; rounded buffer
counts 4805/3203 must not redefine that speed.

If the stage transform is `project_frame=O+T*local_frame`, project sample `n`
requests stage sample coordinate `m=(n-C*O)/T`. Reconstruct the prepared stage
at that exact coordinate. Fractional phase belongs in this sampling operation,
not an offset rounded to an integer native input boundary. Final allocation
still uses the absolute project-frame endpoints rounded once.

## Required plan boundary

The plan must expose an occurrence-bound stage descriptor with its complete
`InstancePath`, child identity and selected range, output duration, pitch policy,
exact speed, and the stage's own transform captured before descending into its
child. Retain its intrinsic processing extent separately from outer crops and
the current query. A bounded child-signal query on a declared rational grid must
stop descent at nested Preserve stages so their prepared output remains a
continuous signal. Existing root leaf queries alone cannot supply this contract.

An outer crop or random seek must reproduce the corresponding portion of the
full stage render; it must not redefine an inner stage's history. Integer cache
capacity can enclose a fractional extent but cannot change it. Bind original
identities, speaker layouts, exact grids/extents, ordered stage recipes, context
policies and engine versions in prepared-cache provenance. The current native
1,048,576-frame input limit applies to every intermediate; reject excess until
a qualified long-input strategy exists, rather than resetting arbitrary chunks.

Required integration evidence includes fractional input origins, Preserve across
source seams, nested Preserve transients, mixed Preserve/FollowSpeed order,
outer-crop/seek parity, repeat and override identities, half-sample ties and
budget/cancellation failures. These checks remain outstanding.
